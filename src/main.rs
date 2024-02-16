mod db;

use core::panic;

use anyhow::Context as _;
use db::get_quiz;
use serde_json;
use serenity::all::{CommandDataOptionValue, CommandInteraction, Interaction};
use serenity::builder::{
    CreateCommand, CreateCommandOption, CreateInteractionResponse, CreateInteractionResponseMessage,
};
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::{all::GuildId, async_trait};
use shuttle_secrets::SecretStore;
use shuttle_serenity::SerenityService;
use sqlx::{Executor, PgPool};
use tracing::{error, info};

use crate::db::{delete_quiz, new_quiz};

struct Bot {
    database: PgPool,
    guild_id: String,
}

impl Bot {
    async fn command_new(&self, ctx: &Context, command: &CommandInteraction) {
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
            let cardname: String = document
                .select(&name_selector)
                .next()
                .unwrap()
                .text()
                .collect::<Vec<_>>()[2]
                .trim()
                .to_string();
            let text_selector =
                scraper::Selector::parse("#CardTextSet:nth-child(2) .item_box_text").unwrap();
            let cardtext: String = document
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
                .to_string();
            (cardname, cardtext)
        };

        let (cardname, cardtext) = x();

        let content = match new_quiz(
            &self.database,
            &command.user.id.into(),
            &konami_id.as_i64().unwrap(),
            &cardname,
            &cardtext,
        )
        .await
        {
            Ok(msg) => {
                info!("{}", msg);
                format!(
                    "次のカードテキストを持つ遊戯王カードは？(`/ygoquiz ans` で回答)\n\n{}\n{}",
                    cardtext,
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

    async fn command_ans(&self, ctx: &Context, command: &CommandInteraction) {
        let CommandDataOptionValue::SubCommand(subopt) = &command.data.options[0].value else {
            //unreachable
            panic!()
        };
        let card_name = subopt[0].value.as_str().unwrap();

        info!("Answered: {}", card_name);
        let content = match get_quiz(&self.database, &command.user.id.into()).await {
            Ok(quiz) => {
                if quiz.card_name == card_name {
                    let _ = delete_quiz(&self.database, &command.user.id.into()).await;

                    format!("{}の回答：{}\n\n正解! \n https://www.db.yugioh-card.com/yugiohdb/card_search.action?ope=2&cid={}&request_locale=ja",                        
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

    async fn command_giveup(&self, ctx: &Context, command: &CommandInteraction) {
        info!("Giveup: {}", command.user);
        let content = match get_quiz(&self.database, &command.user.id.into()).await {
            Ok(quiz) => {
                let _ = delete_quiz(&self.database, &command.user.id.into()).await;

                format!("正解は「{}」でした \n https://www.db.yugioh-card.com/yugiohdb/card_search.action?ope=2&cid={}&request_locale=ja", quiz.card_name, quiz.konami_id)
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

    async fn command_help(&self, ctx: &Context, command: &CommandInteraction) {
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
}

#[async_trait]
impl EventHandler for Bot {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            if command.user.bot {
                return;
            }

            info!("Received command interaction: {:#?}", command.data.options);

            match command.data.name.as_str() {
                "ygoquiz" => match command.data.options[0].name.as_str() {
                    "new" => self.command_new(&ctx, &command).await,
                    "ans" => self.command_ans(&ctx, &command).await,
                    "giveup" => self.command_giveup(&ctx, &command).await,
                    "help" => self.command_help(&ctx, &command).await,
                    _ => {}
                },
                _ => {}
            };
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        let guild_id = GuildId::new(self.guild_id.parse().unwrap());

        let commands = guild_id
            .set_commands(
                &ctx.http,
                vec![CreateCommand::new("ygoquiz")
                    .description("Communicate with Yu-gi-oh! quiz bot")
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
                    ))],
            )
            .await
            .unwrap();

        info!("Registered commands: {:#?}", commands);
    }
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_shared_db::Postgres] pool: PgPool,
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_serenity::ShuttleSerenity {
    // Get the discord token set in `Secrets.toml
    let token = secret_store
        .get("DISCORD_TOKEN")
        .context("'DISCORD_TOKEN' was not found")?;
    let guild_id = secret_store
        .get("GUILD_ID")
        .context("'GUILD_ID' was not found")?;

    pool.execute(include_str!("../schema.sql"))
        .await
        .context("failed to run migrations")?;

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let client = Client::builder(&token, intents)
        .event_handler(Bot {
            database: pool,
            guild_id,
        })
        .await
        .expect("Err creating client");

    Ok(SerenityService(client))
}
