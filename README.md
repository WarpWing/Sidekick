# Sidekick

A seriously lightweight Discord Bot written using [Serenity](https://github.com/serenity-rs/serenity/tree/current) running on [Alpine Linux](https://www.alpinelinux.org/). Mostly testing + various functions. 

<img width="1134" height="94" alt="image" src="https://github.com/user-attachments/assets/b94fa8f5-db48-4f83-b7db-0e6ec88276d9" />


## Commands

- `/gather host event location hours`: schedule an event and open RSVPs
- `/cancel message_id`: cancel an event (host or admin only)
- `/help`: what the bot does

To get a message ID for `/cancel`: enable Developer Mode
(User Settings -> Advanced -> Developer Mode), then right-click the event message
and "Copy Message ID."

## Setup

1. Create an application + bot at the [Discord Developer Portal](https://discord.com/developers/applications).
2. Invite it with the `bot` and `applications.commands` scopes, and the
   **Manage Roles** permission. Drag the bot's role *above* where event roles
   will sit, or it can't create/assign them.
3. Set your bot token as an environment variable: `DISCORD_TOKEN`.

## Running with Docker

    docker build -t sidekick .
    docker run -d \
      --name sidekick \
      -e DISCORD_TOKEN="your_token" \
      -v sidekick-data:/data \
      --restart unless-stopped \
      sidekick

## Running locally

    export DISCORD_TOKEN="your_token"
    cargo run

Note: Default behavior will have the database write to `sidekick.db` in the current directory. Override that path
with by exporting `DATABASE_PATH` with a new value. 

