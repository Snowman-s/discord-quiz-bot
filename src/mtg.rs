pub mod db;

use std::collections::HashMap;

use reqwest::Client;
use serde_json::json;
use serenity::{
    all::{CommandDataOption, CommandDataOptionValue, CommandInteraction, Mentionable},
    builder::{CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage},
    client::Context,
};
use tracing::{error, info};

use crate::Bot;

use crate::common::roughly_card_name_equal;
use crate::mtg::db::{get_quiz, new_quiz};

pub(crate) fn create_subcommand(c: CreateCommandOption) -> CreateCommandOption {
    c.description("Communicate with Magic:the Gathering! quiz bot")
        .add_sub_option(
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
            )
            .add_sub_option(CreateCommandOption::new(
                serenity::all::CommandOptionType::Boolean,
                "rare",
                "If true, only rare cards will be selected",
            )),
        )
}

pub(crate) async fn receive_command(bot: &Bot, ctx: &Context, command: CommandInteraction) {
    let CommandDataOptionValue::SubCommandGroup(after_mtg) = &command.data.options[0].value else {
        //unreachable
        panic!()
    };
    let subc = after_mtg[0].name.as_str();

    info!(subc);
    let result = match subc {
        "new" => command_new(bot, ctx, &command, &after_mtg[0]).await,
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
    command_data_option: &CommandDataOption,
) -> Result<String, String> {
    let CommandDataOptionValue::SubCommand(subopt) = &command_data_option.value else {
        //unreachable
        panic!()
    };
    let mut cmd_arg_map = HashMap::new();
    for opt in subopt {
        cmd_arg_map.insert(opt.name.as_str(), &opt.value);
    }

    let format = cmd_arg_map
        .get("format")
        .map(|res| res.as_str().unwrap())
        .unwrap_or("");
    info!("format = {}", format);
    let rare_mode = cmd_arg_map
        .get("rare")
        .map(|res| res.as_bool().unwrap())
        .unwrap_or(false);
    info!("rare_mode = {}", rare_mode);

    let query = [
        "lang:japanese",
        match format {
            "standard" => "f:standard",
            "pioneer" => "f:pioneer",
            "modern" => "f:modern",
            "eternal" | _ => "",
        },
        if rare_mode { "r>=r" } else { "" },
    ]
    .into_iter()
    .filter(|o| !o.is_empty())
    .collect::<Vec<_>>()
    .join(" ");

    let client = Client::new();

    let card: serde_json::Value = serde_json::from_str(
        &client
            .get("https://api.scryfall.com/cards/random")
            .header("Accept", "application/json")
            .header("User-Agent", "ygo-quiz-bot/1.0")
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
                "次のカードテキストを持つ Magic のカードは？(`/quiz ans` で回答)\n\n{}\n{}",
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
            if roughly_card_name_equal(card_name, &quiz.card_name, &quiz.english_name) {
                let _ = crate::db::delete_quiz(&bot.database, &command.user.id.into()).await;

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
            "データベースでエラーが発生しました (`/quiz new` は実行しましたか？) : {}",
            err.to_string()
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

            format!(
                "正解は「{}」（{}）でした \n {}",
                quiz.card_name, quiz.english_name, quiz.scryfall_uri
            )
        }
        Err(err) => format!(
            "データベースでエラーが発生しました (`/quiz mtg new` は実行しましたか？) : {}",
            err.to_string()
        ),
    };

    Ok(content)
}
