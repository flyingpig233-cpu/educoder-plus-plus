//! Small diagnostic CLI. Never pass credentials on the command line.
use educoder_core::{
    EducoderClient, Session, SessionVault,
    models::{HomeworkKind, PageRequest},
};
use std::io::Write;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        println!(
            "用法: crawl import <cookies.txt> | me | courses | homeworks <courseId> [page] | challenges <shixunId> | games <myshixunId> | task <gameId> | code <gameId> <path> | report <reportId> | logout\n读取命令可通过 EDUCODER_COOKIES 环境变量指定 Cookie 文件；否则使用系统保存的会话。"
        );
        return Ok(());
    };
    let arg = |index: usize| args.get(index).map(String::as_str).ok_or("缺少命令参数");
    let vault = SessionVault::new("default");
    if command == "logout" {
        vault.clear().await?;
        println!("已清除保存的登录会话");
        return Ok(());
    }
    if command == "import" {
        let session = Session::from_file(arg(1)?).await?;
        let client = EducoderClient::new(session.clone())?;
        let user = client.validate_session().await?;
        vault.save(&session).await?;
        println!("已保存 {} 的登录会话", user.display_name);
        return Ok(());
    }
    let session = if let Some(path) = std::env::var_os("EDUCODER_COOKIES") {
        Session::from_file(path).await?
    } else {
        vault
            .load()
            .await?
            .ok_or("尚无登录会话，请先 import 或设置 EDUCODER_COOKIES")?
    };
    let client = EducoderClient::new(session)?;
    let output = match command {
        "me" => serde_json::to_string_pretty(&client.get_user_info().await?)?,
        "courses" => serde_json::to_string_pretty(&client.get_courses().await?)?,
        "homeworks" => serde_json::to_string_pretty(
            &client
                .get_homeworks(
                    arg(1)?.parse()?,
                    HomeworkKind::Shixun,
                    PageRequest {
                        page: args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(1),
                        ..Default::default()
                    },
                )
                .await?,
        )?,
        "challenges" => serde_json::to_string_pretty(&client.get_challenges(arg(1)?).await?)?,
        "games" => serde_json::to_string_pretty(&client.get_my_challenges(arg(1)?).await?)?,
        "task" => serde_json::to_string_pretty(&client.get_task(arg(1)?).await?)?,
        "report" => serde_json::to_string_pretty(&client.get_work_report(arg(1)?.parse()?).await?)?,
        "code" => {
            let file = client.get_file_content(arg(1)?, arg(2)?).await?;
            std::io::stdout().write_all(&file.content)?;
            return Ok(());
        }
        _ => return Err("未知命令".into()),
    };
    println!("{output}");
    Ok(())
}
