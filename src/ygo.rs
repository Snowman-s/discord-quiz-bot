mod db;

use serenity::{
    all::{CommandDataOptionValue, CommandInteraction, EditInteractionResponse, Mentionable},
    builder::{CreateCommand, CreateCommandOption},
    client::Context,
};
use tracing::{error, info};

use crate::Bot;

use crate::common::roughly_card_name_equal;
use crate::ygo::db::{delete_quiz, get_quiz, new_quiz};

pub(crate) fn create_command(c: CreateCommand) -> CreateCommand {
    c.description("Communicate with Yu-gi-oh! quiz bot")
        .add_option(CreateCommandOption::new(
            serenity::all::CommandOptionType::SubCommand,
            "new",
            "Start Yu-gi-oh! quiz",
        ))
        .add_option(
            CreateCommandOption::new(
                serenity::all::CommandOptionType::SubCommand,
                "ans",
                "Answer to Yu-gi-oh! quiz",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    serenity::all::CommandOptionType::String,
                    "card_name",
                    "The card name",
                )
                .required(true),
            ),
        )
        .add_option(CreateCommandOption::new(
            serenity::all::CommandOptionType::SubCommand,
            "giveup",
            "Giveup Yu-gi-oh! quiz",
        ))
        .add_option(CreateCommandOption::new(
            serenity::all::CommandOptionType::SubCommand,
            "help",
            "Help about Yu-gi-oh! quiz bot",
        ))
}

pub(crate) async fn receive_command(bot: &Bot, ctx: &Context, command: CommandInteraction) {
    command.defer(&ctx.http).await.unwrap();

    let subc = command.data.options[0].name.as_str();
    let result = match subc {
        "new" => command_new(&bot, &ctx, &command).await,
        "ans" => command_ans(&bot, &ctx, &command).await,
        "giveup" => command_giveup(&bot, &ctx, &command).await,
        "help" => command_help(&bot, &ctx, &command).await,
        _ => Err(format!("Unknown Command: {}", subc)),
    };

    let msg = match result {
        Ok(msg) => msg,
        Err(msg) => {
            error!(msg);
            msg
        }
    };

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

async fn command_new(
    bot: &Bot,
    _: &Context,
    command: &CommandInteraction,
) -> Result<String, String> {
    let card: serde_json::Value = serde_json::from_str(
        &reqwest::get("https://db.ygoprodeck.com/api/v7/randomcard.php")
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    info!("id (not konami_id) = {:?}", card.get("id"));
    let card: serde_json::Value = serde_json::from_str::<serde_json::Value>(
        &reqwest::get(format!(
            "https://db.ygoprodeck.com/api/v7/cardinfo.php?misc=yes&id={}",
            card.get("id").ok_or("API 応答の解析失敗")?
        ))
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
            .split("\n")
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
                "次のカードテキストを持つ遊戯王カードは？(`/ygoquiz ans` で回答)\n\n{}\n{}",
                card_text,
                card.get("card_images")
                    .and_then(|c| c.as_array())
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("image_url_cropped"))
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
            )
        }
        Err(err) => {
            error!("{}", err);
            format!("データベースでエラーが発生しました：{}", err)
        }
    };

    info!(content);
    Ok(content)
}

async fn command_ans(
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
                let _ = delete_quiz(&bot.database, &command.user.id.into()).await;

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
            "データベースでエラーが発生しました (`/ygoquiz new` は実行しましたか？) : {}",
            err.to_string()
        ),
    };

    Ok(content)
}

async fn command_giveup(
    bot: &Bot,
    _: &Context,
    command: &CommandInteraction,
) -> Result<String, String> {
    info!("Giveup: {}", command.user);
    let content = match get_quiz(&bot.database, &command.user.id.into()).await {
        Ok(quiz) => {
            let _ = delete_quiz(&bot.database, &command.user.id.into()).await;

            format!("正解は「{}」（{}）でした \n https://www.db.yugioh-card.com/yugiohdb/card_search.action?ope=2&cid={}&request_locale=ja", quiz.card_name, quiz.card_name_ruby, quiz.konami_id)
        }
        Err(err) => format!(
            "データベースでエラーが発生しました (`/ygoquiz new` は実行しましたか？) : {}",
            err.to_string()
        ),
    };

    Ok(content)
}

async fn command_help(_: &Bot, _: &Context, _: &CommandInteraction) -> Result<String, String> {
    Ok("Help:\n".to_owned()
        + "遊戯王カードのカードテキストを出し、カードテキストから名前を当てる遊びです。\n"
        + "ユーザーごとに別の問題に取り組むことができます。\n\n"
        + "Commands:\n"
        + "- `/ygoquiz new` - 開始\n"
        + "- `/ygoquiz ans <card_name>` - 回答\n"
        + "- `/ygoquiz giveup` - 問題を諦める\n"
        + "- `/ygoquiz help` - このヘルプを表示\n")
}
