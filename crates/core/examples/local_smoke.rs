//! Compile/run synthetic samples using available local toolchains. No platform access.
use educoder_core::{
    models::CodeFile,
    runner::{Sample, Toolchains, inspect_toolchains, run_samples},
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tools = Toolchains::default();
    let statuses = inspect_toolchains(&tools).await;
    let demos = [
        (
            "C",
            "main.c",
            "#include <stdio.h>\nint main(){int a,b;scanf(\"%d%d\",&a,&b);printf(\"%d\\n\",a+b);}",
        ),
        (
            "C++",
            "main.cpp",
            "#include <iostream>\nint main(){int a,b;std::cin>>a>>b;std::cout<<a+b<<std::endl;}",
        ),
        (
            "Rust",
            "main.rs",
            "use std::io::{self,Read}; fn main(){let mut s=String::new();io::stdin().read_to_string(&mut s).unwrap();println!(\"{}\",s.split_whitespace().map(|s|s.parse::<i32>().unwrap()).sum::<i32>());}",
        ),
        ("Python", "main.py", "print(sum(map(int, input().split())))"),
        (
            "JavaScript",
            "main.js",
            "console.log(require('fs').readFileSync(0,'utf8').trim().split(/\\s+/).map(Number).reduce((a,b)=>a+b,0));",
        ),
        (
            "Java",
            "Main.java",
            "import java.util.Scanner; public class Main {public static void main(String[] args){Scanner s=new Scanner(System.in);System.out.println(s.nextInt()+s.nextInt());}}",
        ),
    ];
    for (name, path, source) in demos {
        if !statuses.iter().any(|s| s.language == name && s.available) {
            println!("SKIP: {name}, toolchain unavailable");
            continue;
        }
        let result = run_samples(
            &[CodeFile {
                path: path.into(),
                content: source.as_bytes().to_vec(),
            }],
            path,
            &[Sample {
                name: "sum".into(),
                input: "1 2\n".into(),
                expected: Some("3\n".into()),
            }],
            &tools,
            Duration::from_secs(5),
        )
        .await?;
        if result.samples.first().and_then(|s| s.passed) != Some(true) {
            eprintln!("FAILED: {name}; {result:?}");
            return Err("local toolchain verification failed".into());
        }
        println!("PASS: {name} compile/run sample");
    }
    Ok(())
}
