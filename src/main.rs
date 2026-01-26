use clap::Parser;
use regex::Regex;
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};
use tabwriter::TabWriter;

#[derive(Debug, clap::Parser)]
#[clap(name = "gix-scripts")]
pub struct Args {
    #[clap(long)]
    pub git_work_tree: PathBuf,

    #[clap(long)]
    pub baseline_executable: PathBuf,

    #[clap(long)]
    pub comparison_executable: PathBuf,

    #[clap(long)]
    pub args: Option<String>,

    #[clap(long)]
    pub skip: Option<usize>,

    #[clap(long)]
    pub take: Option<usize>,
}

static GIT_BLAME_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| Regex::new(r"\^?([0-9a-f]+) (?:([^(^)]+)\s+)?(\(.* \d+\)) (.*)").unwrap());

static GIX_BLAME_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| Regex::new(r"([0-9a-f]+) (\d+) (?:\S+ )?(\d+) (.*)").unwrap());

fn regex_for_executable(executable: &Path) -> Result<&'static LazyLock<Regex>, ()> {
    if executable.ends_with("git") {
        return Ok(&GIT_BLAME_RE);
    } else if executable.ends_with("gix") {
        return Ok(&GIX_BLAME_RE);
    }

    Err(())
}

impl Args {
    fn git_dir(&self) -> PathBuf {
        self.git_work_tree.join(".git")
    }
}

fn main() {
    let args: Args = Args::parse_from(std::env::args_os());

    let git_dir = args.git_work_tree.join(".git");

    let output = Command::new("git")
        .env("GIT_DIR", git_dir)
        .args(["ls-files", "--format", "%(path) %(eolinfo:index)"])
        .output()
        .expect("failed to run git ls-files");

    let filenames: Vec<_> = output
        .stdout
        .lines()
        .filter_map(|line| {
            let line = line.expect("could not decode line");
            let parts: Vec<_> = line.split_whitespace().collect();

            match parts[..] {
                [filename, attr] if !attr.contains("-text") => Some(filename.to_string()),
                _ => None,
            }
        })
        .collect();

    let number_of_files = filenames.len();

    let skip = args.skip.unwrap_or(0);
    let take = args.take.unwrap_or(number_of_files);

    println!(
        "{} files to run blame for, skip {}, take {}",
        number_of_files, skip, take
    );
    println!("comparing blames");

    let mut stdout = std::io::stdout();

    let baseline_regex = regex_for_executable(&args.baseline_executable)
        .expect("baseline executable is not associated with a regex");
    let comparison_regex = regex_for_executable(&args.comparison_executable)
        .expect("comparison executable is not associated with a regex");

    let outcomes: Vec<_> = filenames
        .iter()
        .skip(skip)
        .take(take)
        .map(|filename| {
            let result = compare_two_blames(&args, baseline_regex, comparison_regex, filename);

            let char = match result {
                Outcome::BlamesMatch { .. } => '.',
                _ => 'x',
            };

            print!("{char}");
            let _ = stdout.flush();

            (filename, result)
        })
        .collect();

    println!();

    let number_of_matches = outcomes
        .iter()
        .filter(|(_, outcome)| matches!(outcome, Outcome::BlamesMatch { .. }))
        .count();
    let number_of_non_matches = outcomes.len() - number_of_matches;

    if number_of_non_matches == 0 {
        println!("\ndone, all blames matched");
    } else {
        println!(
            "\ndone, number of matches: {}, number of non-matches: {}",
            number_of_matches, number_of_non_matches
        );

        let number_of_non_matches_to_show = number_of_non_matches.clamp(0, 256);

        println!(
            "\nnon-matches (showing {number_of_non_matches_to_show} of {number_of_non_matches}):\n"
        );

        let mut tw = TabWriter::new(vec![]);

        for (filename, outcome) in outcomes
            .iter()
            .filter(|(_, outcome)| !matches!(outcome, Outcome::BlamesMatch { .. }))
            .take(number_of_non_matches_to_show)
        {
            writeln!(&mut tw, "{filename}\t{outcome}").unwrap();
        }

        tw.flush().unwrap();

        print!("{}", String::from_utf8(tw.into_inner().unwrap()).unwrap());

        println!("\nsummary\n");

        let (number_of_matching_lines, number_of_non_matching_lines) =
            outcomes
                .iter()
                .fold((0, 0), |acc, (_, outcome)| match outcome {
                    Outcome::BlamesMatch {
                        number_of_matching_lines,
                    } => (acc.0 + number_of_matching_lines, acc.1),
                    Outcome::LinesPartiallyMatched {
                        number_of_matching_lines,
                        non_matching_lines,
                    } => (
                        acc.0 + number_of_matching_lines,
                        acc.1 + non_matching_lines.len(),
                    ),
                    _ => acc,
                });

        let all_lines = number_of_matching_lines + number_of_non_matching_lines;
        let percentage = (number_of_matching_lines as f32) / (all_lines as f32) * 100.0;

        println!("{number_of_matching_lines}/{all_lines} {percentage:.2} %");
    }
}

#[derive(Debug)]
enum Outcome {
    DifferingLineNumbers,
    BlamesMatch {
        number_of_matching_lines: usize,
    },
    LineDidNotMatchPattern,
    LinesPartiallyMatched {
        number_of_matching_lines: usize,
        non_matching_lines: Vec<usize>,
    },
    FailedToRunExecutable,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::DifferingLineNumbers => "differing line numbers".fmt(f),
            Outcome::BlamesMatch {
                number_of_matching_lines: matching_lines,
            } => format!("blames match\t#{matching_lines}").fmt(f),
            Outcome::LineDidNotMatchPattern => "a line did not match the pattern".fmt(f),
            Outcome::LinesPartiallyMatched {
                number_of_matching_lines,
                non_matching_lines,
            } => {
                let all_lines = number_of_matching_lines + non_matching_lines.len();
                let percentage: f32 =
                    (*number_of_matching_lines as f32) / (all_lines as f32) * 100.0;

                let non_matching_lines = non_matching_lines
                    .iter()
                    .take(10)
                    .map(|line_number| line_number.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                format!(
                    "hashes partially matched\t{number_of_matching_lines}/{all_lines}\t{percentage:.2} %\t{non_matching_lines}",
                )
                .fmt(f)
            }
            Outcome::FailedToRunExecutable => "failed to run executable".fmt(f),
        }
    }
}

fn compare_two_blames<T: AsRef<str>>(
    args: &Args,
    baseline_regex: &LazyLock<Regex>,
    comparison_regex: &LazyLock<Regex>,
    filename: T,
) -> Outcome {
    let extra_args = args.args.clone().unwrap_or("".to_string());

    let baseline_output = Command::new("bash")
        .env("GIT_DIR", args.git_dir())
        .env("GIT_WORK_TREE", args.git_work_tree.clone())
        .arg("-c")
        .arg(format!(
            "{} blame {} {}",
            args.baseline_executable.to_string_lossy(),
            extra_args,
            filename.as_ref()
        ))
        .output()
        .expect("failed to run baseline executable");

    if !baseline_output.status.success() {
        println!("{baseline_output:?}");

        return Outcome::FailedToRunExecutable;
    }

    let comparison_output = Command::new("bash")
        .env("GIT_DIR", args.git_dir())
        .env("GIT_WORK_TREE", args.git_work_tree.clone())
        .arg("-c")
        .arg(format!(
            "{} blame {} {}",
            args.comparison_executable.to_string_lossy(),
            extra_args,
            filename.as_ref()
        ))
        .output()
        .expect("failed to run comparison executable");

    if !comparison_output.status.success() {
        println!("{comparison_output:?}");

        return Outcome::FailedToRunExecutable;
    }

    let baseline_lines: Vec<_> = baseline_output
        .stdout
        .lines()
        .map(|line| line.expect("could not decode line"))
        .collect();
    let comparison_lines: Vec<_> = comparison_output
        .stdout
        .lines()
        .map(|line| line.expect("could not decode line"))
        .collect();

    if baseline_lines.len() != comparison_lines.len() {
        return Outcome::DifferingLineNumbers;
    }

    let mut number_of_matching_lines = 0;
    let mut non_matching_lines: Vec<usize> = Vec::new();

    for (line_number, (baseline_line, comparison_line)) in
        baseline_lines.into_iter().zip(comparison_lines).enumerate()
    {
        let Some(baseline_captures) = baseline_regex.captures(&baseline_line) else {
            return Outcome::LineDidNotMatchPattern;
        };
        let Some(comparison_captures) = comparison_regex.captures(&comparison_line) else {
            return Outcome::LineDidNotMatchPattern;
        };

        let baseline_hash = &baseline_captures[1];
        let comparison_hash = &comparison_captures[1];

        if !baseline_hash.starts_with(comparison_hash)
            && !comparison_hash.starts_with(baseline_hash)
        {
            non_matching_lines.push(line_number);
        } else {
            number_of_matching_lines += 1;
        }
    }

    if non_matching_lines.is_empty() {
        Outcome::BlamesMatch {
            number_of_matching_lines,
        }
    } else {
        Outcome::LinesPartiallyMatched {
            number_of_matching_lines,
            non_matching_lines,
        }
    }
}
