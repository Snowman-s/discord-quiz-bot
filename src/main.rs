mod common;
mod db;
mod mtg;
mod ygo;

use anyhow::Context as _;
use db::get_quiz_type;
use serenity::all::{
    CommandInteraction, CreateCommandOption, CreateInteractionResponse,
    CreateInteractionResponseMessage, Interaction,
};
use serenity::builder::CreateCommand;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::{all::GuildId, async_trait};
use shuttle_secrets::SecretStore;
use shuttle_serenity::SerenityService;
use sqlx::{Executor, PgPool};
use tracing::{error, info};

struct Bot {
    database: PgPool,
    guild_id: String,
}

impl Bot {
    async fn command_help(&self, ctx: &Context, command: &CommandInteraction) {
        command
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new().content(
                        "Help:\n".to_owned()
                            + "クイズを出すので回答してください\n"
                            + "ユーザーごとに別の問題に取り組むことができます。\n\n"
                            + "Commands:\n"
                            + "- `/quiz <type> new` - 開始\n"
                            + "- `/quiz ans <answer>` - 回答\n"
                            + "- `/quiz giveup` - 問題を諦める\n"
                            + "- `/quiz help` - このヘルプを表示\n",
                    ),
                ),
            )
            .await
            .unwrap();
    }

    async fn command_general(&self, ctx: &Context, command: &CommandInteraction) {
        let msg = match get_quiz_type(&self.database, &command.user.id.into()).await {
            Ok(quiz_type) => match command.data.options[0].name.as_str() {
                "ans" => {
                    let result = match quiz_type {
                        db::QuizType::Ygo => ygo::command_ans(self, ctx, command).await,
                        db::QuizType::Mtg => mtg::command_ans(self, ctx, command).await,
                    };
                    match result {
                        Ok(msg) => msg,
                        Err(msg) => {
                            error!(msg);
                            msg
                        }
                    }
                }
                "giveup" => {
                    let result = match quiz_type {
                        db::QuizType::Ygo => ygo::command_giveup(self, ctx, command).await,
                        db::QuizType::Mtg => mtg::command_giveup(self, ctx, command).await,
                    };
                    match result {
                        Ok(msg) => msg,
                        Err(msg) => {
                            error!(msg);
                            msg
                        }
                    }
                }
                _ => "謎のコマンド".into(),
            },
            Err(err) => format!(
                "データベースでエラーが発生しました (`/quiz <タイプ> new` は実行しましたか？) : {}",
                err.to_string()
            ),
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
}

#[async_trait]
impl EventHandler for Bot {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::Command(command) = interaction {
            if command.user.bot || command.data.name.as_str() != "quiz" {
                return;
            }

            info!("Received command interaction: {:#?}", command.data.options);

            match command.data.options[0].name.as_str() {
                "ygo" => ygo::receive_command(&self, &ctx, command).await,
                "mtg" => mtg::receive_command(&self, &ctx, command).await,
                "help" => self.command_help(&ctx, &command).await,
                _ => self.command_general(&ctx, &command).await,
            };
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        let guild_id = GuildId::new(self.guild_id.parse().unwrap());

        let commands = guild_id
            .set_commands(
                &ctx.http,
                vec![CreateCommand::new("quiz")
                    .description("Communicate with quiz bot")
                    .add_option(
                        CreateCommandOption::new(
                            serenity::all::CommandOptionType::SubCommand,
                            "ans",
                            "Answer to quiz",
                        )
                        .add_sub_option(
                            CreateCommandOption::new(
                                serenity::all::CommandOptionType::String,
                                "answer",
                                "The answer",
                            )
                            .required(true),
                        ),
                    )
                    .add_option(CreateCommandOption::new(
                        serenity::all::CommandOptionType::SubCommand,
                        "giveup",
                        "Giveup quiz",
                    ))
                    .add_option(CreateCommandOption::new(
                        serenity::all::CommandOptionType::SubCommand,
                        "help",
                        "Help of quiz bot",
                    ))
                    .add_option(ygo::create_subcommand(CreateCommandOption::new(
                        serenity::all::CommandOptionType::SubCommandGroup,
                        "ygo",
                        "",
                    )))
                    .add_option(mtg::create_subcommand(CreateCommandOption::new(
                        serenity::all::CommandOptionType::SubCommandGroup,
                        "mtg",
                        "",
                    )))],
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
