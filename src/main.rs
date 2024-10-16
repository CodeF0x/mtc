use clap::Parser;
use glob::glob;
use std::ffi::OsStr;
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Parser, Debug, Clone)]
#[command(version, about)]
struct CmdArgs {
    /// the amount of threads you want to utilize. most systems can handle 2. go higher if you have a powerful computer.
    #[arg(short, long, default_value_t = 2)]
    thread_count: u8,

    /// options you want to pass to ffmpeg. for the output file name, use --output
    #[arg(short, long, allow_hyphen_values = true)]
    ffmpeg_options: String,

    /// the directory with all files you want to process. supports unix globs
    #[arg(short, long)]
    input_directory: String,

    /// Specify the output file pattern. Use placeholders to customize file paths:
    ///
    /// {{dir}}  - Original file's directory structure
    ///
    /// {{name}} - Original file's name (without extension)
    ///
    /// {{ext}}  - Original file's extension
    ///
    /// Example: /destination/{{dir}}/{{name}}_transcoded.{{ext}}
    ///
    /// Outputs the file in /destination, mirroring the original structure and keeping both the file extension and name, while adding _transcoded to the name.
    #[arg(short, long)]
    output: String,
    // {{ext}} -> extension, {{name}} filename without extension, {{dir}} -> directory structure from starting point to file, {{parent}} -> parent directory of starting point
}

fn main() {
    let cmd_args = CmdArgs::parse();

    let paths = Arc::new(Mutex::new(match glob(&cmd_args.input_directory) {
        Ok(paths) => paths.filter_map(Result::ok).collect::<Vec<PathBuf>>(),
        Err(err) => {
            eprintln!("{}", err.msg);
            std::process::exit(1);
        }
    }));

    let mut thread_handles = vec![];

    for thread in 0..cmd_args.thread_count {
        let paths: Arc<Mutex<Vec<PathBuf>>> = Arc::clone(&paths);
        let args = cmd_args.clone();

        let handle = thread::spawn(move || loop {
            let path_to_process = {
                let mut queue = paths.lock().unwrap();

                queue.pop()
            };

            match path_to_process {
                Some(path) => {
                    println!("[THREAD {thread}] -- Processing {}", path.display());
                    let split_options = &mut args.ffmpeg_options.split(' ').collect::<Vec<&str>>();

                    let mut final_file_name = args
                        .output
                        .replace("{{ext}}", path.extension().unwrap().to_str().unwrap());
                    final_file_name = final_file_name
                        .replace("{{name}}", &path.file_stem().unwrap().to_str().unwrap());
                    final_file_name = final_file_name.replace(
                        "{{dir}}",
                        &path.parent().unwrap_or(Path::new("")).to_str().unwrap(),
                    );
                    final_file_name = final_file_name.replace(
                        "{{parent}}",
                        &path
                            .parent()
                            .unwrap_or(Path::new(""))
                            .file_name()
                            .unwrap_or(OsStr::new(""))
                            .to_str()
                            .unwrap_or(""),
                    );
                    let final_path_parent = Path::new(&final_file_name).parent().unwrap();

                    if !final_path_parent.exists() {
                        match create_dir_all(final_path_parent) {
                            Ok(_) => {}
                            Err(err) => {
                                eprintln!(
                                    "[THREAD {thread}] -- Could not create directory structure for file {}",
                                    final_file_name
                                );
                                eprintln!("{}", err)
                            }
                        }
                    }

                    if let Ok(output) = Command::new("ffmpeg")
                        .args(["-i", path.to_str().unwrap()])
                        .args(split_options)
                        .arg(&final_file_name)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .output()
                    {
                        if output.status.success() {
                            println!("[THREAD {thread}] -- Success, saving to {final_file_name}");
                        } else {
                            eprintln!("[THREAD {thread}] -- Error!");
                            eprintln!(
                                "[THREAD {thread}] -- Error is: {}",
                                String::from_utf8_lossy(&output.stderr)
                            );
                            eprintln!("[THREAD {thread}] -- Continuing with next task if there's more to do...");
                        }
                    } else {
                        eprintln!("[THREAD {thread}] -- There was an error running ffmpeg. Please check if it's correctly installed and working as intended.");
                    }
                }
                None => {
                    break;
                }
            }
        });

        thread_handles.push(handle);
    }

    for handle in thread_handles {
        handle.join().unwrap();
    }
}
