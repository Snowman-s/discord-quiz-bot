mod common;
mod mtg;
mod ygo;

use anyhow::Context as _;
use serenity::all::Interaction;
use serenity::builder::CreateCommand;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use serenity::{all::GuildId, async_trait};
use shuttle_secrets::SecretStore;
use shuttle_serenity::SerenityService;
use sqlx::{Executor, PgPool};
use tracing::info;

struct Bot {
    database: PgPool,
    guild_id: String,
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
                "ygoquiz" => ygo::receive_command(&self, &ctx, command).await,
                "mtgquiz" => mtg::receive_command(&self, &ctx, command).await,
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
                vec![
                    ygo::create_command(CreateCommand::new("ygoquiz")),
                    mtg::create_command(CreateCommand::new("mtgquiz")),
                ],
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
