use serenity::{
    all::{CommandDataOptionValue, CommandInteraction, Mentionable},
    builder::{
        CreateCommand, CreateCommandOption, CreateInteractionResponse,
        CreateInteractionResponseMessage,
    },
    client::Context,
};
use tracing::{error, info};

use crate::{db::get_quiz, Bot};

use crate::common::roughly_card_name_equal;
use crate::db::{delete_quiz, new_quiz};

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
    match command.data.options[0].name.as_str() {
        "new" => command_new(&bot, &ctx, &command).await,
        "ans" => command_ans(&bot, &ctx, &command).await,
        "giveup" => command_giveup(&bot, &ctx, &command).await,
        "help" => command_help(&bot, &ctx, &command).await,
        _ => {}
    };
}

async fn command_new(bot: &Bot, ctx: &Context, command: &CommandInteraction) {
    let card: serde_json::Value = serde_json::from_str(
        &reqwest::get("https://db.ygoprodeck.com/api/v7/randomcard.php")
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    let card: serde_json::Value = serde_json::from_str::<serde_json::Value>(
        &reqwest::get(format!(
            "https://db.ygoprodeck.com/api/v7/cardinfo.php?misc=yes&id={}",
            card.get("id").unwrap()
        ))
        .await
        .unwrap()
        .text()
        .await
        .unwrap(),
    )
    .unwrap()
    .get("data")
    .unwrap()
    .as_array()
    .unwrap()[0]
        .clone();
    let konami_id = card.get("misc_info").unwrap().as_array().unwrap()[0]
        .get("konami_id")
        .unwrap();
    info!("konami_id = {}", konami_id);
    let url = format!(
        "https://www.db.yugioh-card.com/yugiohdb/card_search.action?ope=2&cid={}&request_locale=ja",
        konami_id
    );

    info!("konami_db_url = {}", url);
    let html = reqwest::get(url).await.unwrap().text().await.unwrap();

    //https://github.com/causal-agent/scraper/issues/75
    let x = || {
        let document = scraper::Html::parse_document(&html);
        let name_selector = scraper::Selector::parse("#cardname h1").unwrap();
        let card_names = document
            .select(&name_selector)
            .next()
            .unwrap()
            .text()
            .collect::<Vec<_>>();
        let card_name: String = card_names[2].trim().to_string();
        let card_name_ruby: String = card_names[1].trim().to_string();
        let text_selector =
            scraper::Selector::parse("#CardTextSet:nth-child(2) .item_box_text").unwrap();
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
        (card_name, card_name_ruby, card_text)
    };

    let (card_name, card_name_ruby, card_text) = x();

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
                card.get("card_images").unwrap().as_array().unwrap()[0]
                    .get("image_url_cropped")
                    .unwrap()
                    .as_str()
                    .unwrap()
            )
        }
        Err(err) => {
            error!("{}", err);
            format!("データベースでエラーが発生しました：{}", err)
        }
    };

    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content(content),
            ),
        )
        .await
        .unwrap();
}

async fn command_ans(bot: &Bot, ctx: &Context, command: &CommandInteraction) {
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

    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content(content),
            ),
        )
        .await
        .unwrap();
}

async fn command_giveup(bot: &Bot, ctx: &Context, command: &CommandInteraction) {
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

    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content(content),
            ),
        )
        .await
        .unwrap();
}

async fn command_help(_: &Bot, ctx: &Context, command: &CommandInteraction) {
    command
      .create_response(
          &ctx.http,
          CreateInteractionResponse::Message(
              CreateInteractionResponseMessage::new().content(
                "Help:\n".to_owned()+
                "遊戯王カードのカードテキストを出し、カードテキストから名前を当てる遊びです。\n"+
                "ユーザーごとに別の問題に取り組むことができます。\n\n"+
                "Commands:\n"+
                "- `/ygoquiz new` - 開始\n"+
                "- `/ygoquiz ans <card_name>` - 回答\n"+
                "- `/ygoquiz giveup` - 問題を諦める\n"+
                "- `/ygoquiz help` - このヘルプを表示\n"  
              ),
          ),
      )
      .await
      .unwrap();
}