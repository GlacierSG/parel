use std::process::Command;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::fs::File;
use std::io::{self, BufRead};
use std::thread;
use clap::Parser;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};


fn execute_command(command: &str, no_output: bool) {
    match Command::new("sh")
        .arg("-c")
        .arg(command)
        .output() {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !no_output {
                    print!("{}", stdout);
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !no_output {
                    eprint!("{}", stderr.red());
                }
            }
        }
        Err(err) => {
            eprintln!("{} Failed to execute `{}`: {}", "warning:".yellow().bold(), command, err);
        }
    }
}

fn precompute_template(command: &String, loaded_wordlist: &Vec<(String, Vec<String>)>) -> Vec<(usize, String)> {
    let mut template: Vec<(usize, String)> = vec![];
    let mut tmp = String::new();
    let mut i = 0;
    while i < command.len() {
        let mut found = false;
        for j in 0..loaded_wordlist.len() {
            let identifier = &loaded_wordlist[j].0;
            if command[i..].starts_with(identifier) {
                template.push((j, tmp)); 
                tmp = String::new();
                i += identifier.len();
                found = true;
                break;
            }
        }
        if !found {
            tmp.push_str(&command[i..i+1]);
            i += 1;
        }
    }
    template.push((0, tmp)); 
    template
}
fn gen_command(template: &Vec<(usize, String)>, idx: usize, loaded_wordlist: &Vec<(String, Vec<String>)>, wordlist_lengths: &Vec<usize>) -> String {
    let mut command = String::new();
    let idxs = product(idx, &wordlist_lengths);
    for tvalue in template {
        command.push_str(&tvalue.1);
        command.push_str(&loaded_wordlist[tvalue.0].1[idxs[tvalue.0]]);
    }
    command.push_str(&template[template.len()-1].1);
    command
}

fn read_lines<P>(filename: P) -> io::Result<Vec<String>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    let lines: Vec<String> = io::BufReader::new(file).lines().filter_map(Result::ok).collect();
    Ok(lines)
}

fn product(nth: usize, sizes: &Vec<usize>) -> Vec<usize> {
    let mut out = vec![];
    let mut nth = nth;
    for s in sizes {
        out.push(nth % s);
        nth /= s;
    }
    return out;
}

#[derive(Parser, Debug)]
#[command(name = "parel", version = env!("CARGO_PKG_VERSION"), about = "Parallization CLI tool", disable_version_flag = true)]
struct Cli {
    command: String,
    #[arg(short, long, default_value_t=10, help="Number of threads")]
    threads: usize,
    #[arg(short, long, default_value=None, help="Show nth command that will be executed")]
    show: Option<usize>,
    #[arg(short, long, help="A file and an identifier used in command [example: abc.txt:foo]")]
    file: Vec<String>,
    #[arg(long, help="Don't show command stdout or stderr")]
    no_output: bool,
    #[arg(short, long, help="Enable progress bar")]
    progress: bool,
    #[arg(long, action = clap::builder::ArgAction::Version)]
    version: (),
}

fn main() {
    let args = Cli::parse();
    let command = args.command;

    let mut files: Vec<(String, String)> = vec![];

    for line in args.file {
        let mut identifier: String = "".to_string();
        let mut path: String = "".to_string();
        for j in (0..line.len()).rev() {
            if line[j..j+1] == *":" {
                identifier = line[j+1..].to_string();
                path = line[..j].to_string();
            }
        }
        if identifier.is_empty() {
            eprintln!("{} Missing file name, example: '-f {}:foo'", "error:".red().bold(), line);
            std::process::exit(1);
        }
        if files.iter().any(|(f, _)| f == &identifier) {
            eprintln!("{} file identifier '{}' aready exists", "error:".red().bold(), identifier);
            std::process::exit(1);
        }
        if !Path::new(&path).exists() {
            eprintln!("{} file '{}' does not exist", "error:".red().bold(), path);
            std::process::exit(1);
        }

        if !command.contains(&identifier) {
            eprintln!("{} worldlist name '{}' is not in command", "error:".red().bold(), identifier);
            std::process::exit(1);
        }
        files.push((identifier.clone(), path));
    }
   
    let mut total_words = 1;
    let mut loaded_wordlist: Vec<(String, Vec<String>)> = vec![];
    let mut wordlist_lengths: Vec<usize> = vec![];
    for (identifier, path) in files {
        let lines = read_lines(&path).expect(&format!("{} Could not read {}", "error:".red().bold(), &path));

        total_words *= lines.len();
        wordlist_lengths.push(lines.len());
        loaded_wordlist.push((identifier, lines));
    }


    let template = precompute_template(&command, &loaded_wordlist);

    if let Some(show) = args.show {
        if show >= total_words {
            eprintln!("{} show parameter {} cannot be more than {}", "error:".red().bold(), show, total_words);
            std::process::exit(1);
        }
        let command = gen_command(&template, show, &loaded_wordlist, &wordlist_lengths);
        println!("{}", command);
        std::process::exit(0);
    }
    let progress_bar = if args.progress {
        let mut pb = ProgressBar::new(total_words as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({percent}%)")
                .unwrap()
                .progress_chars("#>-")
        );
        Some(Arc::new(Mutex::new(pb)))
    }
    else { None };

    let next_job = Arc::new(Mutex::new(0));

    let wordlist_lengths = Arc::new(wordlist_lengths);
    let loaded_wordlist = Arc::new(loaded_wordlist);
    let template = Arc::new(template);


    let mut threads = vec![];
    for _ in 0..args.threads {
        let wordlist_lengths = wordlist_lengths.clone();
        let loaded_wordlist = loaded_wordlist.clone();
        let template = template.clone();
        let next_job = next_job.clone();
        let progress_bar = progress_bar.clone();
    
        threads.push(thread::spawn(move || {
            loop {
                let job = {
                    let mut job = next_job.lock().unwrap();
                    let out = *job;
                    if out > total_words {
                        break;
                    }
                    else {
                        *job += 1;
                        out
                    }
                };
                let command = gen_command(&template, job, &loaded_wordlist, &wordlist_lengths);
                execute_command(&command, args.no_output);

                if let Some(ref pb) = progress_bar {
                    pb.lock().unwrap().inc(1);
                }
            }
        }));
    }

    for thread in threads {
        let _ = thread.join();
    }
    if let Some(ref pb) = progress_bar {
        pb.lock().unwrap().finish();
    }
}
