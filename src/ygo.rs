pub mod db;

use serenity::{
    all::{
        CommandDataOptionValue, CommandInteraction, CreateAttachment,
        CreateInteractionResponseFollowup, EditInteractionResponse, Mentionable,
    },
    builder::CreateCommandOption,
    client::Context,
    futures::TryFutureExt,
};
use tracing::{error, info};

use crate::Bot;

use crate::common::roughly_card_name_equal;
use crate::ygo::db::{get_quiz, new_quiz};

pub(crate) fn create_subcommand(c: CreateCommandOption) -> CreateCommandOption {
    c.description("Communicate with Yu-gi-oh! quiz bot")
        .add_sub_option(
            CreateCommandOption::new(
                serenity::all::CommandOptionType::SubCommand,
                "new",
                "Start Yu-gi-oh! quiz",
            )
            .add_sub_option(CreateCommandOption::new(
                serenity::all::CommandOptionType::String,
                "fname",
                "If specified, only cards with it in the card name will be asked",
            )),
        )
}

pub(crate) async fn receive_command(bot: &Bot, ctx: &Context, command: CommandInteraction) {
    command.defer(&ctx.http).await.unwrap();

    let CommandDataOptionValue::SubCommandGroup(after_ygo) = &command.data.options[0].value else {
        //unreachable
        panic!()
    };

    let subc = after_ygo[0].name.as_str();
    let result = match subc {
        "new" => {
            info!("{:?}", after_ygo[0].value);
            let CommandDataOptionValue::SubCommand(ref params) = after_ygo[0].value else {
                //unreachable
                panic!()
            };
            let fname = if !params.is_empty() {
                params[0].value.as_str()
            } else {
                None
            };
            info!(fname);
            command_new(bot, ctx, &command, fname).await
        }
        _ => Err(format!("Unknown Command: {}", subc)),
    };

    match result {
        Ok((msg, attachments)) => {
            match command
                .edit_response(&ctx.http, EditInteractionResponse::new().content(msg))
                .and_then(|_msg| {
                    command.create_followup(
                        &ctx.http,
                        CreateInteractionResponseFollowup::new().files(attachments),
                    )
                })
                .await
            {
                Ok(_) => {}
                Err(err) => {
                    error!("{}", err)
                }
            }
        }
        Err(msg) => {
            error!(msg);
            match command
                .edit_response(&ctx.http, EditInteractionResponse::new().content(msg))
                .await
            {
                Ok(_) => {}
                Err(err) => {
                    error!("{}", err)
                }
            }
        }
    }
}

async fn command_new(
    bot: &Bot,
    _: &Context,
    command: &CommandInteraction,
    op_fname: Option<&str>,
) -> Result<(String, Vec<CreateAttachment>), String> {
    let client = reqwest::Client::new();

    let mut query = vec![
        ("num", "1"),
        ("offset", "0"),
        ("sort", "random"),
        ("cachebust", ""),
        ("misc", "yes"),
    ];

    if let Some(fname) = op_fname {
        query.push(("fname", fname));
    }

    let card: serde_json::Value = serde_json::from_str::<serde_json::Value>(
        &client
            .get("https://db.ygoprodeck.com/api/v7/cardinfo.php")
            .query(&query)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?
    .get("data")
    .ok_or("API 応答の解析失敗")?
    .as_array()
    .ok_or("API 応答の解析失敗")?[0]
        .clone();
    let konami_id = card
        .get("misc_info")
        .ok_or("API 応答の解析失敗")?
        .as_array()
        .ok_or("API 応答の解析失敗")?[0]
        .get("konami_id")
        .ok_or("API 応答の解析失敗")?;
    info!("konami_id = {}", konami_id);
    let url = format!(
        "https://www.db.yugioh-card.com/yugiohdb/card_search.action?ope=2&cid={}&request_locale=ja",
        konami_id
    );

    info!("konami_db_url = {}", url);
    let html = reqwest::get(url)
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    //https://github.com/causal-agent/scraper/issues/75
    let x = || ->Result<
      (
        std::string::String,
        std::string::String,
        std::string::String,
      ),
      String,
    > {
        let document = scraper::Html::parse_document(&html);
        let name_selector = scraper::Selector::parse("#cardname h1").map_err(|e| e.to_string())?;
        let card_names = document
            .select(&name_selector)
            .next()
            .ok_or("遊戯王DBの解析失敗")?
            .text()
            .collect::<Vec<_>>();
        let card_name: String = card_names[2].trim().to_string();
        let card_name_ruby: String = card_names[1].trim().to_string();
        let text_selector = scraper::Selector::parse("#CardTextSet:nth-child(2) .item_box_text")
            .map_err(|e| e.to_string())?;
        let card_text: String = document
            .select(&text_selector)
            .map(|t| t.inner_html())
            .collect::<Vec<_>>()
            .join("")
            .split('\n')
            .filter_map(|card_line: &str| {
                if card_line.contains("<div class=\"text_title\">") {
                    None
                } else if card_line.contains("</div>") {
                    None
                } else if card_line.trim() == "" {
                    None
                } else {
                    Some(card_line.trim())
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            .replace("<br>", "\n")
            .replace(&card_name, "<カード名>")
            .to_string();
        Ok((card_name, card_name_ruby, card_text))
    };

    let (card_name, card_name_ruby, card_text) = x()?;

    let response = reqwest::get(
        card.get("card_images")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
            .and_then(|c| c.get("image_url_cropped"))
            .and_then(|c| c.as_str())
            .unwrap_or(""),
    )
    .await
    .map_err(|_err| "画像取得エラー")?;
    let img_bytes = response.bytes().await.unwrap();

    let content = match new_quiz(
        &bot.database,
        &command.user.id.into(),
        &konami_id.as_i64().unwrap(),
        &card_name,
        &card_name_ruby,
        &card_text,
    )
    .await
    {
        Ok(msg) => {
            info!("{}", msg);
            format!(
                "{}次のカードテキストを持つ遊戯王カードは？(`/quiz ans` で回答)\n\n{}",
                if let Some(fname) = op_fname {
                    format!("カード名に「{}」が含まれている、", fname)
                } else {
                    "".to_owned()
                },
                card_text
            )
        }
        Err(err) => {
            error!("{}", err);
            format!("データベースでエラーが発生しました：{}", err)
        }
    };

    info!(content);
    Ok((
        content,
        vec![CreateAttachment::bytes(img_bytes, "image.jpg")],
    ))
}

pub async fn command_ans(
    bot: &Bot,
    _: &Context,
    command: &CommandInteraction,
) -> Result<String, String> {
    let CommandDataOptionValue::SubCommand(subopt) = &command.data.options[0].value else {
        //unreachable
        panic!()
    };
    let card_name = subopt[0].value.as_str().unwrap();

    info!("Answered: {}", card_name);
    let content = match get_quiz(&bot.database, &command.user.id.into()).await {
        Ok(quiz) => {
            if roughly_card_name_equal(card_name, &quiz.card_name, &quiz.card_name_ruby) {
                let _ = crate::db::delete_quiz(&bot.database, &command.user.id.into()).await;

                format!("{}の回答：{}\n\n正解！ \n https://www.db.yugioh-card.com/yugiohdb/card_search.action?ope=2&cid={}&request_locale=ja",                        
                command.user.mention(),
                card_name,
                quiz.konami_id
              )
            } else {
                format!(
                    "{}の回答：{}\n\n不正解...",
                    command.user.mention(),
                    card_name
                )
            }
        }
        Err(err) => format!(
            "データベースでエラーが発生しました (`/quiz ygo new` は実行しましたか？) : {}",
            err
        ),
    };

    Ok(content)
}

pub async fn command_giveup(
    bot: &Bot,
    _: &Context,
    command: &CommandInteraction,
) -> Result<String, String> {
    info!("Giveup: {}", command.user);
    let content = match get_quiz(&bot.database, &command.user.id.into()).await {
        Ok(quiz) => {
            let _ = crate::db::delete_quiz(&bot.database, &command.user.id.into()).await;

            format!("正解は「{}」（{}）でした \n https://www.db.yugioh-card.com/yugiohdb/card_search.action?ope=2&cid={}&request_locale=ja", quiz.card_name, quiz.card_name_ruby, quiz.konami_id)
        }
        Err(err) => format!(
            "データベースでエラーが発生しました (`/quiz new` は実行しましたか？) : {}",
            err
        ),
    };

    Ok(content)
}
