//! Read-only public archive diagnostic. Prints counts only, never answer contents.
use educoder_core::answers::{AnswerClient, AnswerQuery};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: answers <course-id> <homework-id> <position>".into());
    }
    let result = AnswerClient::new()?
        .lookup(
            &AnswerQuery {
                course_id: args[0].parse()?,
                homework_id: args[1].parse()?,
                position: args[2].parse()?,
            },
            true,
        )
        .await?;
    println!(
        "found={}, files={}, cached={}, stale={}",
        result.answer.is_some(),
        result.answer.as_ref().map_or(0, |a| a.files.len()),
        result.cached,
        result.stale
    );
    if result.stale {
        return Err("live fetch failed; using cached answer".into());
    }
    Ok(())
}
