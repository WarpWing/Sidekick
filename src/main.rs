use std::env;
use std::time::Duration;

use chrono::{NaiveDate, NaiveTime, TimeZone};
use chrono_tz::America::New_York;
use serenity::async_trait;
use serenity::builder::{
    CreateCommand, CreateCommandOption, CreateEmbed, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateSelectMenu, CreateSelectMenuKind,
    CreateSelectMenuOption, EditMessage, EditRole,
};
use serenity::gateway::ActivityData;
use serenity::model::application::{Command, CommandOptionType, Interaction};
use serenity::model::channel::{Reaction, ReactionType};
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, GuildId, MessageId, RoleId};
use serenity::model::Colour;
use serenity::prelude::*;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};

const ADMIN_ID: i64 = 232239924462616578;

struct Handler {
    db: SqlitePool,
}

fn render_list(raw: &str) -> String {
    let users: Vec<&str> = raw.split(',').filter(|s| !s.is_empty()).collect();
    if users.is_empty() {
        "Nobody yet.".to_string()
    } else {
        users.iter().map(|u| format!("<@{u}>")).collect::<Vec<_>>().join("\n")
    }
}

fn count(raw: &str) -> usize {
    raw.split(',').filter(|s| !s.is_empty()).count()
}

fn build_embed(host: i64, name: &str, location: &str, starts: i64, going: &str, not_going: &str) -> CreateEmbed {
    CreateEmbed::new()
        .title(name.to_string())
        .description(format!(
            "<@{host}> is hosting a **{name}**.\nLocation: {location}\nStarts <t:{starts}:F> (<t:{starts}:R>)\n\nReact below if you're interested!"
        ))
        .field(format!("Going ({})", count(going)), render_list(going), true)
        .field(format!("Not Going ({})", count(not_going)), render_list(not_going), true)
        .colour(Colour::from_rgb(88, 101, 242))
}

fn parse_start(date: Option<&str>, time: &str) -> Result<i64, String> {
    let today = chrono::Utc::now().with_timezone(&New_York).date_naive();

    let date = match date {
        Some(d) => NaiveDate::parse_from_str(d.trim(), "%Y-%m-%d")
            .map_err(|_| "I couldn't read that date. Use YYYY-MM-DD, like 2026-06-10.".to_string())?,
        None => today,
    };

    let t = time.trim();
    let parsed_time = NaiveTime::parse_from_str(t, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(t, "%I:%M%p"))
        .or_else(|_| NaiveTime::parse_from_str(t, "%I%p"))
        .map_err(|_| "I couldn't read that time. Try something like 19:30 or 7:30pm.".to_string())?;

    let naive = date.and_time(parsed_time);

    let dt = match New_York.from_local_datetime(&naive).single() {
        Some(dt) => dt,
        None => return Err("That time is ambiguous or invalid because of daylight saving. Try a different time.".to_string()),
    };

    let ts = dt.timestamp();
    if ts <= now_unix() {
        return Err("That start time is in the past. Pick a future time.".to_string());
    }
    Ok(ts)
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        ctx.set_activity(Some(ActivityData::playing("Winning pickleball tournaments")));

        let gather = CreateCommand::new("gather")
            .description("Schedule an event and let people RSVP")
            .add_option(
                CreateCommandOption::new(CommandOptionType::User, "host", "Who is hosting").required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "event", "Name of the event").required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "location", "Where it's happening").required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "time", "Start time in Eastern, e.g. 19:30 or 7:30pm").required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "date", "Date as YYYY-MM-DD (defaults to today)").required(false),
            );

        Command::create_global_command(&ctx.http, gather).await.unwrap();
        Command::create_global_command(&ctx.http, CreateCommand::new("cancel").description("Cancel an event you're hosting"))
            .await
            .unwrap();
        Command::create_global_command(&ctx.http, CreateCommand::new("help").description("What this bot does"))
            .await
            .unwrap();

        let db = self.db.clone();
        let http = ctx.http.clone();
        tokio::spawn(async move {
            loop {
                let now = now_unix();
                let rows = sqlx::query("SELECT message_id, guild_id, role_id FROM events WHERE starts_at <= ?")
                    .bind(now)
                    .fetch_all(&db)
                    .await
                    .unwrap_or_default();

                for row in rows {
                    let message_id: i64 = row.get("message_id");
                    let guild_id: i64 = row.get("guild_id");
                    let role_id: i64 = row.get("role_id");

                    GuildId::new(guild_id as u64)
                        .delete_role(&http, RoleId::new(role_id as u64))
                        .await
                        .ok();
                    sqlx::query("DELETE FROM events WHERE message_id = ?")
                        .bind(message_id)
                        .execute(&db)
                        .await
                        .ok();
                    println!("event {message_id} started, role removed");
                }

                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        });
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(cmd) => match cmd.data.name.as_str() {
                "help" => {
                    let embed = CreateEmbed::new()
                        .title("Sidekick")
                        .description(
                            "Sidekick is a small bot for organizing events. Use the commands below to get started.\n\n\
                             Built with Rust + Serenity and ran on [Alpine Linux](https://www.alpinelinux.org/). \
                             Check out the code [here](https://github.com/WarpWing/Sidekick)."
                        )
                        .field("/gather", "Pick a host, name an event, set a location, and give a start time in Eastern. You can add a date as YYYY-MM-DD, and if you leave it out the event is scheduled for today. People react to RSVP and the lists update live. Anyone going gets the event role, and that role is deleted automatically once the event starts.", false)
                        .field("/cancel", "Pick an event from the dropdown to cancel it. Only the host or an admin can do this.", false)
                        .field("/help", "This shows you what the bot does.", false)
                        .colour(Colour::from_rgb(88, 101, 242));

                    cmd.create_response(&ctx.http, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().embed(embed),
                    )).await.ok();
                }

                "gather" => {
                    let Some(guild_id) = cmd.guild_id else {
                        cmd.create_response(&ctx.http, CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new().content("This only works in a server.").ephemeral(true),
                        )).await.ok();
                        return;
                    };

                    let opts = &cmd.data.options;
                    let host = opts.iter().find(|o| o.name == "host").and_then(|o| o.value.as_user_id()).unwrap();
                    let name = opts.iter().find(|o| o.name == "event").and_then(|o| o.value.as_str()).unwrap().to_string();
                    let location = opts.iter().find(|o| o.name == "location").and_then(|o| o.value.as_str()).unwrap().to_string();
                    let time = opts.iter().find(|o| o.name == "time").and_then(|o| o.value.as_str()).unwrap();
                    let date = opts.iter().find(|o| o.name == "date").and_then(|o| o.value.as_str());

                    let starts_at = match parse_start(date, time) {
                        Ok(ts) => ts,
                        Err(msg) => {
                            cmd.create_response(&ctx.http, CreateInteractionResponse::Message(
                                CreateInteractionResponseMessage::new().content(msg).ephemeral(true),
                            )).await.ok();
                            return;
                        }
                    };

                    let role = match guild_id.create_role(&ctx.http, EditRole::new().name(&name)).await {
                        Ok(r) => r,
                        Err(e) => {
                            println!("couldn't make the role: {e:?}");
                            return;
                        }
                    };

                    cmd.create_response(&ctx.http, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().embed(build_embed(host.get() as i64, &name, &location, starts_at, "", "")),
                    )).await.ok();

                    let msg = cmd.get_response(&ctx.http).await.unwrap();
                    msg.react(&ctx.http, ReactionType::Unicode("✅".into())).await.ok();
                    msg.react(&ctx.http, ReactionType::Unicode("❌".into())).await.ok();

                    sqlx::query(
                        "INSERT INTO events (message_id, guild_id, channel_id, role_id, host, name, location, starts_at, going, not_going)
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, '', '')",
                    )
                    .bind(msg.id.get() as i64)
                    .bind(guild_id.get() as i64)
                    .bind(cmd.channel_id.get() as i64)
                    .bind(role.id.get() as i64)
                    .bind(host.get() as i64)
                    .bind(&name)
                    .bind(&location)
                    .bind(starts_at)
                    .execute(&self.db)
                    .await
                    .unwrap();
                }

                "cancel" => {
                    let Some(guild_id) = cmd.guild_id else {
                        cmd.create_response(&ctx.http, CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new().content("This only works in a server.").ephemeral(true),
                        )).await.ok();
                        return;
                    };

                    let rows = sqlx::query("SELECT message_id, name, location, starts_at FROM events WHERE guild_id = ? ORDER BY starts_at")
                        .bind(guild_id.get() as i64)
                        .fetch_all(&self.db)
                        .await
                        .unwrap_or_default();

                    if rows.is_empty() {
                        cmd.create_response(&ctx.http, CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new().content("There are no events to cancel right now.").ephemeral(true),
                        )).await.ok();
                        return;
                    }

                    let options: Vec<CreateSelectMenuOption> = rows
                        .iter()
                        .take(25)
                        .map(|row| {
                            let mid: i64 = row.get("message_id");
                            let name: String = row.get("name");
                            let location: String = row.get("location");
                            CreateSelectMenuOption::new(name, mid.to_string()).description(location)
                        })
                        .collect();

                    let menu = CreateSelectMenu::new("cancel_select", CreateSelectMenuKind::String { options })
                        .placeholder("Pick an event to cancel");

                    cmd.create_response(&ctx.http, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .content("Which event do you want to cancel?")
                            .select_menu(menu)
                            .ephemeral(true),
                    )).await.ok();
                }

                _ => {}
            },

            Interaction::Component(component) => {
                if component.data.custom_id != "cancel_select" {
                    return;
                }

                let serenity::model::application::ComponentInteractionDataKind::StringSelect { values } = &component.data.kind else {
                    return;
                };
                let Some(raw) = values.first() else { return };
                let Ok(mid) = raw.parse::<i64>() else { return };

                let row = sqlx::query("SELECT guild_id, channel_id, role_id, host, name FROM events WHERE message_id = ?")
                    .bind(mid)
                    .fetch_optional(&self.db)
                    .await
                    .ok()
                    .flatten();

                let Some(row) = row else {
                    component.create_response(&ctx.http, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().content("That event no longer exists.").ephemeral(true),
                    )).await.ok();
                    return;
                };

                let host: i64 = row.get("host");
                let caller = component.user.id.get() as i64;
                if caller != host && caller != ADMIN_ID {
                    component.create_response(&ctx.http, CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new().content("Only the host can cancel this event.").ephemeral(true),
                    )).await.ok();
                    return;
                }

                let guild_id: i64 = row.get("guild_id");
                let channel_id: i64 = row.get("channel_id");
                let role_id: i64 = row.get("role_id");
                let name: String = row.get("name");

                GuildId::new(guild_id as u64).delete_role(&ctx.http, RoleId::new(role_id as u64)).await.ok();
                sqlx::query("DELETE FROM events WHERE message_id = ?").bind(mid).execute(&self.db).await.ok();

                let cancelled = CreateEmbed::new()
                    .title(format!("{name} has been cancelled."))
                    .description("This event has been cancelled by the host.")
                    .colour(Colour::from_rgb(220, 50, 50));

                if let Ok(mut msg) = ChannelId::new(channel_id as u64).message(&ctx.http, MessageId::new(mid as u64)).await {
                    msg.edit(&ctx.http, EditMessage::new().embed(cancelled)).await.ok();
                }

                component.create_response(&ctx.http, CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new().content(format!("You cancelled {name}.")).ephemeral(true),
                )).await.ok();
            }

            _ => {}
        }
    }

    async fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        self.rsvp(&ctx, reaction, true).await;
    }

    async fn reaction_remove(&self, ctx: Context, reaction: Reaction) {
        self.rsvp(&ctx, reaction, false).await;
    }
}

impl Handler {
    async fn rsvp(&self, ctx: &Context, reaction: Reaction, added: bool) {
        let Some(user) = reaction.user_id else { return };
        if user == ctx.cache.current_user().id {
            return;
        }
        let Some(guild_id) = reaction.guild_id else { return };

        let ReactionType::Unicode(emoji) = &reaction.emoji else { return };
        if emoji != "✅" && emoji != "❌" {
            return;
        }

        let mid = reaction.message_id.get() as i64;
        let row = sqlx::query("SELECT host, name, location, role_id, starts_at, going, not_going FROM events WHERE message_id = ?")
            .bind(mid)
            .fetch_optional(&self.db)
            .await
            .ok()
            .flatten();
        let Some(row) = row else { return };

        let host: i64 = row.get("host");
        let name: String = row.get("name");
        let location: String = row.get("location");
        let role_id: i64 = row.get("role_id");
        let starts_at: i64 = row.get("starts_at");
        let going_raw: String = row.get("going");
        let not_going_raw: String = row.get("not_going");

        let uid = user.get().to_string();
        let strip = |raw: &str, who: &str| {
            raw.split(',').filter(|s| !s.is_empty() && *s != who).collect::<Vec<_>>().join(",")
        };

        let mut going = strip(&going_raw, &uid);
        let mut not_going = strip(&not_going_raw, &uid);

        if added {
            if emoji == "✅" {
                going = if going.is_empty() { uid.clone() } else { format!("{going},{uid}") };
            } else {
                not_going = if not_going.is_empty() { uid.clone() } else { format!("{not_going},{uid}") };
            }
        }

        sqlx::query("UPDATE events SET going = ?, not_going = ? WHERE message_id = ?")
            .bind(&going)
            .bind(&not_going)
            .bind(mid)
            .execute(&self.db)
            .await
            .ok();

        if let Ok(member) = guild_id.member(&ctx.http, user).await {
            let role = RoleId::new(role_id as u64);
            if going.split(',').any(|s| s == uid) {
                member.add_role(&ctx.http, role).await.ok();
            } else {
                member.remove_role(&ctx.http, role).await.ok();
            }
        }

        let embed = build_embed(host, &name, &location, starts_at, &going, &not_going);
        if let Ok(mut msg) = reaction.channel_id.message(&ctx.http, MessageId::new(mid as u64)).await {
            msg.edit(&ctx.http, EditMessage::new().embed(embed)).await.ok();
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let db_path = env::var("DATABASE_PATH").unwrap_or_else(|_| "sidekick.db".to_string());
    let db = SqlitePoolOptions::new()
        .connect(&format!("sqlite:{db_path}?mode=rwc"))
        .await
        .expect("couldn't open database");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS events (
            message_id INTEGER PRIMARY KEY,
            guild_id   INTEGER NOT NULL,
            channel_id INTEGER NOT NULL,
            role_id    INTEGER NOT NULL,
            host       INTEGER NOT NULL,
            name       TEXT NOT NULL,
            location   TEXT NOT NULL,
            starts_at  INTEGER NOT NULL,
            going      TEXT NOT NULL,
            not_going  TEXT NOT NULL
        )",
    )
    .execute(&db)
    .await
    .expect("couldn't create table");

    let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGE_REACTIONS;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler { db })
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {why:?}");
    }
}
