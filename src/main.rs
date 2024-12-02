mod drink_tips;

use std::env;
use std::sync::Arc;
use log::info;
use sqlx::{Pool, Sqlite, SqlitePool};
use teloxide::utils::command::parse_command;
use teloxide::{
    dispatching::dialogue::{
        serializer::{Json},
        ErasedStorage, SqliteStorage, Storage
    },
    prelude::*,
    utils::command::BotCommands,
};
use crate::drink_tips::register_tips;

#[derive(Clone, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum State {
    #[default]
    Start,
    Nicked(String),
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "支持的命令:")]
enum Command {
    #[command(description="开始")]
    Start,
    #[command(description="帮助菜单")]
    Help,
    #[command(description="问候")]
    Hi,
    #[command(description="掷骰子游戏")]
    Dice,
    // /// Handle a username.
    // #[command(alias = "u", description="")]
    // Username(String),
    // /// Handle a username and an age.
    // #[command(parse_with = "split", alias = "ua", hide_aliases, description="")]
    // UsernameAndAge { username: String, age: u8 },
    #[command(description = "喝水水提醒⏰, /drinktips 20 表示每20分钟提醒您喝水")]
    DrinkTips(i32),
}

type MyDialogue = Dialogue<State, ErasedStorage<State>>;
type MyStorage = std::sync::Arc<ErasedStorage<State>>;
type HandlerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting command bot...");

    let bot = Bot::from_env();

    // 初始化数据库
    let storage: MyStorage = SqliteStorage::open("db.sqlite", Json).await.unwrap().erase();

    let sqlite_pool = SqlitePool::connect(&env::var("DATABASE_URL").unwrap()).await.unwrap();

    let handler = Update::filter_message()
        .enter_dialogue::<Message, ErasedStorage<State>, State>()
        .branch(dptree::case![State::Start]
                    .branch(dptree::entry().filter_command::<Command>().endpoint(start_cmd))
                    .branch(dptree::endpoint(start)),
        )
        .branch(dptree::case![State::Nicked(nick)]
                    .branch(dptree::entry().filter_command::<Command>().endpoint(valid_command))
                    .branch(dptree::endpoint(invalid_command)),
        );

    // 定时任务
    drink_tips::schedule(bot.clone(), sqlite_pool.clone());

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![storage, sqlite_pool])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn start(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    if msg.text().is_none() {
        bot.send_message(msg.chat.id, "初次见面，给自己取个称呼吧").await?;
    } else {
        let nickname = msg.text().unwrap();
        bot.send_message(msg.chat.id, format!("好的，以后我就称呼你{}啦; 使用帮助命令 /help 查看更多操作", nickname)).await?;
        dialogue.update(State::Nicked(String::from(nickname))).await?;
    }
    Ok(())
}

async fn start_cmd(bot: Bot, dialogue: MyDialogue, msg: Message, cmd: Command) -> HandlerResult {
    match cmd {
        Command::Start => {
            bot.send_message(msg.chat.id, "初次见面，给自己取个称呼吧").await?;
        }
        _ => {}
    }
    Ok(())
}


async fn valid_command(bot: Bot, dialogue: MyDialogue, nick: String, msg: Message, cmd: Command, database: Pool<Sqlite>) -> HandlerResult {
    match cmd {
        Command::Start => {
            dialogue.reset().await?;
            bot.send_message(msg.chat.id, "让我们重新开始吧! /start").await?;
        }
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
        }
        Command::Hi => {
            bot.send_message(msg.chat.id, format!("Hi! {}", nick)).await?;
        }
        Command::DrinkTips(minute) => {
            register_tips(&bot, nick, msg, minute, database).await?;
        }
        Command::Dice => {
            bot.send_dice(msg.chat.id).await?;
        }

    }
    Ok(())
}

async fn invalid_command(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
    Ok(())
}

