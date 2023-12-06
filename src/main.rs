use anyhow::Context;
use clap::crate_version;
use clap::{App, Arg};
use fastmod::*;
use grep::regex::RegexMatcherBuilder;
use regex::RegexBuilder;
use rprompt::prompt_reply_stderr;

fn fastmod() -> Result<()> {
    let matches = App::new("fastmod")
        .about("fastmod is a fast partial replacement for codemod.")
        .version(crate_version!())
        .long_about(
            "fastmod is a tool to assist you with large-scale codebase refactors
that can be partially automated but still require human oversight and occasional
intervention.

Example: Let's say you're deprecating your use of the <font> tag. From the
command line, you might make progress by running:

  fastmod -m -d www --extensions php,html \\
      '<font *color=\"?(.*?)\"?>(.*?)</font>' \\
      '<span style=\"color: ${1};\">${2}</span>'

For each match of the regex, you'll be shown a colored diff and asked if you
want to accept the change, reject it, or edit the line in question in your
$EDITOR of choice.

NOTE: Whereas codemod uses Python regexes, fastmod uses the Rust regex
crate, which supports a slightly different regex syntax and does not
support look around or backreferences. In particular, use ${1} instead
of \\1 to get the contents of the first capture group, and use $$ to
write a literal $ in the replacement string. See
https://docs.rs/regex#syntax for details.

A consequence of this syntax is that the use of single quotes instead
of double quotes around the replacment text is important, because the
bash shell itself cares about the $ character in double-quoted
strings. If you must double-quote your input text, be careful to
escape $ characters properly!",
        )
        .arg(
            Arg::with_name("multiline")
                .short("m")
                .long("multiline")
                .help("Have regex work over multiple lines (i.e., have dot match newlines)."),
        )
        .arg(
            Arg::with_name("dir")
                .short("d")
                .long("dir")
                .value_name("DIR")
                .help("The path whose descendent files are to be explored.")
                .long_help(
                    "The path whose descendent files are to be explored.
Included as a flag instead of a positional argument for
compatibility with the original codemod.",
                )
                .multiple(true)
                .number_of_values(1),
        )
        .arg(
            Arg::with_name("file_or_dir")
                .value_name("FILE OR DIR")
                .help("Paths whose descendent files are to be explored.")
                .multiple(true)
                .index(3),
        )
        .arg(
            Arg::with_name("ignore_case")
                .short("i")
                .long("ignore-case")
                .help("Perform case-insensitive search."),
        )
        .arg(
            Arg::with_name("extensions")
                .short("e")
                .long("extensions")
                .value_name("EXTENSION")
                .multiple(true)
                .require_delimiter(true)
                .conflicts_with_all(&["glob", "iglob"])
                // TODO: support Unix pattern-matching of extensions?
                .help("A comma-delimited list of file extensions to process."),
        )
        .arg(
            Arg::with_name("glob")
            .short("g")
            .long("glob")
            .value_name("GLOB")
            .multiple(true)
            .conflicts_with("iglob")
            .help("A space-delimited list of globs to process.")
        )
        .arg(
            Arg::with_name("hidden")
                .long("hidden")
                .help("Search hidden files.")
        )
        .arg(
            Arg::with_name("iglob")
            .long("iglob")
            .value_name("IGLOB")
            .multiple(true)
            .help("A space-delimited list of case-insensitive globs to process.")
        )
        .arg(
            Arg::with_name("accept_all")
                .long("accept-all")
                .help("Automatically accept all changes (use with caution)."),
        )
        .arg(
            Arg::with_name("print_changed_files")
                .long("print-changed-files")
                .help("Print the paths of changed files. (Recommended to be combined with --accept-all.)"),
        )
        .arg(
            Arg::with_name("fixed_strings")
                .long("fixed-strings")
                .short("F")
                .help("Treat REGEX as a literal string. Avoids the need to escape regex metacharacters (compare to ripgrep's option of the same name).")
        )
        .arg(
            Arg::with_name("match")
                .value_name("REGEX")
                .help("Regular expression to match.")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("subst")
             // TODO: support empty substitution to mean "open my
             // editor at instances of this regex"?
             .required(true)
             .help("Substitution to replace with.")
             .index(2),
        )
        .get_matches();
    let multiline = matches.is_present("multiline");
    let dirs = {
        let mut dirs: Vec<_> = matches
            .values_of("dir")
            .unwrap_or_default()
            .chain(matches.values_of("file_or_dir").unwrap_or_default())
            .collect();
        if dirs.is_empty() {
            dirs.push(".");
        }
        dirs
    };
    let ignore_case = matches.is_present("ignore_case");
    let file_set = get_file_set(&matches);
    let accept_all = matches.is_present("accept_all");
    let hidden = matches.is_present("hidden");
    let print_changed_files = matches.is_present("print_changed_files");
    let regex_str = matches.value_of("match").expect("match is required!");
    let subst = matches.value_of("subst").expect("subst is required!");
    let (maybe_escaped_regex, subst) = if matches.is_present("fixed_strings") {
        (regex::escape(regex_str), subst.replace("$", "$$"))
    } else {
        (regex_str.to_string(), subst.to_string())
    };
    let regex = RegexBuilder::new(&maybe_escaped_regex)
        .case_insensitive(ignore_case)
        .multi_line(true) // match codemod behavior for ^ and $.
        .dot_matches_new_line(multiline)
        .build()
        .with_context(|| format!("Unable to make regex from {}", regex_str))?;
    if regex.is_match("") {
        let _ = prompt_reply_stderr(&format!(
            "Warning: your regex {:?} matches the empty string. This is probably
not what you want. Press Enter to continue anyway or Ctrl-C to quit.",
            regex,
        ))?;
    }
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(ignore_case)
        .multi_line(true)
        .dot_matches_new_line(multiline)
        .build(&maybe_escaped_regex)?;

    if accept_all {
        Fastmod::run_fast(
            &regex,
            &matcher,
            &subst,
            dirs,
            file_set,
            hidden,
            print_changed_files,
        )
    } else {
        Fastmod::new(accept_all, hidden, print_changed_files)
            .run_interactive(&regex, &matcher, &subst, dirs, file_set)
    }
}

fn main() {
    if let Err(e) = fastmod() {
        eprint!("{:?}", e);
    }
}
