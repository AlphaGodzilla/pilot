use std::ops::Add;
use chrono::{Duration, Utc};
use log::{debug, error, info};
use sqlx::{Pool, Row, Sqlite};
use teloxide::Bot;
use teloxide::prelude::{ChatId, Message, Requester};
use tokio::select;
use tokio::task::JoinHandle;
use crate::{Command, HandlerResult};

pub async fn register_tips(bot: &Bot, nick: String, msg: Message, minute: i32, database: Pool<Sqlite>) -> HandlerResult {
    let chat_id = msg.chat.id;
    let next_ts = (Utc::now() + Duration::minutes(minute as i64));
    // 先尝试更新
    let success = sqlx::query(r#"update drink_tips set minute = ?, next_ts = ? where chat_id = ?"#)
        .bind(minute)
        .bind(next_ts.timestamp_millis())
        .bind(chat_id.0)
        .execute(&database)
        .await?
        .rows_affected() > 0;
    if success {
        let username = msg.chat.username().unwrap_or("unknown");
        info!("用户 nick={}, username={}, chat_id={} 喝水提醒配置更改为, 每{}minute, 下次提醒: {}", &nick, username, chat_id, minute, next_ts.to_rfc3339());
        bot.send_message(msg.chat.id, format!("Hi! {}，我会每隔{}分钟提醒您喝水水哦", nick, minute)).await?;
        return Ok(());
    }
    if !success {
        sqlx::query(r#"INSERT INTO drink_tips (chat_id, minute, next_ts) VALUES (?, ?, ?)"#)
            .bind(chat_id.0)
            .bind(minute)
            .bind(next_ts.timestamp_millis())
            .execute(&database)
            .await?;
        let username = msg.chat.username().unwrap_or("unknown");
        info!("用户 nick={}, username={}, chat_id={} 新的喝水提醒, 每{}minute，下次提醒: {}", &nick, username, chat_id, minute, next_ts.to_rfc3339());
    }
    bot.send_message(msg.chat.id, format!("Hi! {}，我会每隔{}分钟提醒您喝水水哦", nick, minute)).await?;
    Ok(())
}

pub fn schedule(bot: Bot, database: Pool<Sqlite>) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("启动drink_tips定时任务");
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        interval.tick().await;
        loop {
            let check = async {
                interval.tick().await;  // 等待下一个间隔点
                debug!("检查drink_tip");
                let result = sqlx::query(r#"select chat_id, minute from drink_tips where next_ts <= ?"#)
                    .bind(Utc::now().timestamp_millis())
                    .fetch_all(&database)
                    .await;
                match result {
                    Ok(rows) => {
                        for row in rows {
                            let chat_id: i64 = row.get("chat_id");
                            let minute: i64 = row.get("minute");
                            if let Err(err) = bot.send_message(ChatId(chat_id), "Hi亲，该喝水水了哦！").await {
                                error!("发送提醒异常, {:?}", err);
                            } else {
                                if let Err(err) = sqlx::query(r#"update drink_tips set next_ts = ? where chat_id = ?"#)
                                    .bind((Utc::now() + Duration::minutes(minute)).timestamp_millis())
                                    .bind(chat_id)
                                    .execute(&database)
                                    .await
                                {
                                    error!("drink_tips更新next_ts异常, {:?}", err);
                                }
                            }
                        }
                    }
                    Err(err) => {
                        error!("drink_tips定时任务执行异常, {:?}", err);
                    }
                }
            };
            // _ => tokio::signal::ctrl_c().await.expect("Failed to listen for ^C")
            select! {
                cancel = tokio::signal::ctrl_c() => {
                    cancel.expect("Failed to listen for ^C");
                    break;
                },
                _ = check => {
                }
            }
        }
    })
}