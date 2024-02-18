mod db;

use reqwest::Client;
use serde_json::json;
use serenity::{
    all::{CommandDataOptionValue, CommandInteraction, Mentionable},
    builder::{
        CreateCommand, CreateCommandOption, CreateInteractionResponse,
        CreateInteractionResponseMessage,
    },
    client::Context,
};
use tracing::{error, info};

use crate::Bot;

use crate::common::roughly_card_name_equal;
use crate::mtg::db::{delete_quiz, get_quiz, new_quiz};

pub(crate) fn create_command(c: CreateCommand) -> CreateCommand {
    c.description("Communicate with Magic:the Gathering! quiz bot")
        .add_option(
            CreateCommandOption::new(
                serenity::all::CommandOptionType::SubCommand,
                "new",
                "Start Magic:the Gathering quiz",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    serenity::all::CommandOptionType::String,
                    "format",
                    "The format (question range)",
                )
                .add_string_choice("スタンダード", "standard")
                .add_string_choice("パイオニア", "pioneer")
                .add_string_choice("モダン", "modern")
                .add_string_choice("エターナル", "eternal")
                .required(true),
            ),
        )
        .add_option(
            CreateCommandOption::new(
                serenity::all::CommandOptionType::SubCommand,
                "ans",
                "Answer to Magic:the Gathering quiz",
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
            "Giveup Magic:the Gathering quiz",
        ))
        .add_option(CreateCommandOption::new(
            serenity::all::CommandOptionType::SubCommand,
            "help",
            "Help about Magic:the Gathering quiz bot",
        ))
}

pub(crate) async fn receive_command(bot: &Bot, ctx: &Context, command: CommandInteraction) {
    let subc = command.data.options[0].name.as_str();
    info!(subc);
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

    command
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new().content(msg),
            ),
        )
        .await
        .unwrap();
}

async fn command_new(
    bot: &Bot,
    _: &Context,
    command: &CommandInteraction,
) -> Result<String, String> {
    let CommandDataOptionValue::SubCommand(subopt) = &command.data.options[0].value else {
        //unreachable
        panic!()
    };
    let format = subopt[0].value.as_str().unwrap();
    info!("format = {}", format);

    let query = match format {
        "standard" => "lang:japanese, f:standard",
        "pioneer" => "lang:japanese, f:pioneer",
        "modern" => "lang:japanese, f:modern",
        "eternal" | _ => "lang:japanese",
    };

    let client = Client::new();

    let card: serde_json::Value = serde_json::from_str(
        &client
            .get("https://api.scryfall.com/cards/random")
            .query(&json!({
              "q": query
            }))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let card_name = card
        .get("printed_name")
        .ok_or("API応答の解析失敗")?
        .as_str()
        .ok_or("API応答の解析失敗")?;

    let card_text = card
        .get("printed_text")
        .ok_or("API応答の解析失敗")?
        .as_str()
        .ok_or("API応答の解析失敗")?
        .replace(&card_name, "<カード名>");

    let image_uri = card
        .pointer("/image_uris/art_crop")
        .and_then(|t| t.as_str())
        .unwrap_or("");

    let content = match new_quiz(
        &bot.database,
        &command.user.id.into(),
        card.pointer("/scryfall_uri")
            .ok_or("API応答の解析失敗")?
            .as_str()
            .ok_or("API応答の解析失敗")?,
        card_name,
        card.get("name")
            .ok_or("API応答の解析失敗")?
            .as_str()
            .ok_or("API応答の解析失敗")?,
        &card_text,
    )
    .await
    {
        Ok(msg) => {
            info!("{}", msg);
            format!(
                "次のカードテキストを持つ Magic のカードは？(`/mtgquiz ans` で回答)\n\n{}\n{}",
                card_text, image_uri
            )
        }
        Err(err) => {
            error!("{}", err);
            format!("データベースでエラーが発生しました：{}", err)
        }
    };

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
            if roughly_card_name_equal(card_name, &quiz.card_name, &quiz.english_name) {
                let _ = delete_quiz(&bot.database, &command.user.id.into()).await;

                format!(
                    "{}の回答：{}\n\n正解！ \n {}",
                    command.user.mention(),
                    card_name,
                    quiz.scryfall_uri
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
            "データベースでエラーが発生しました (`/mtgquiz new` は実行しましたか？) : {}",
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

            format!(
                "正解は「{}」（{}）でした \n {}",
                quiz.card_name, quiz.english_name, quiz.scryfall_uri
            )
        }
        Err(err) => format!(
            "データベースでエラーが発生しました (`/ygoquiz new` は実行しましたか？) : {}",
            err.to_string()
        ),
    };

    Ok(content)
}

async fn command_help(_: &Bot, _: &Context, _: &CommandInteraction) -> Result<String, String> {
    Ok("".to_string())
}
