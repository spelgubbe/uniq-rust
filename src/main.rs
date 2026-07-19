use std::env;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, StdinLock, StdoutLock, Write, stderr, stdin, stdout};
use std::process::ExitCode;

struct UniqArguments {
    // argument -c to uniq. Write the count of each streak.
    count: bool,
    // argument -u to uniq. Renamed here because it's not unique.
    unique: bool,
    // argument -d / --repeated.
    repeated: bool
}

enum OptionError {
    InvalidOption(String),
    IncompatibleOptions (String)
}

fn parse_dash_args (args: Vec<&String>) -> Result<UniqArguments, OptionError> {
    // parse -u / --unique or -d / --repeated into something structured...
    let mut count_arg: Option<&str> = None;
    let mut repeated_arg: Option<&str> = None;
    let mut unique_arg: Option<&str> = None;
    for arg in args {
        let trimmed_arg = arg.trim();
        match trimmed_arg {
            "-u" | "--unique" => if unique_arg.is_none() { unique_arg = Some (trimmed_arg)},
            "-d" | "--repeated" => if repeated_arg.is_none() { repeated_arg = Some(trimmed_arg) },
            "-c" | "--count" => if count_arg.is_none() { count_arg = Some (trimmed_arg)},
            _ => {
                // unexpected to have an unmatched argument
                return Err (OptionError::InvalidOption (trimmed_arg.to_string()))
            }
        }
    }
    // validation to prevent combination of -u and -d
    if unique_arg.is_some() && repeated_arg.is_some() {
        let mut err_msg: String = String::new();
        err_msg.push_str(unique_arg.unwrap());
        err_msg.push_str (" and ");
        err_msg.push_str(repeated_arg.unwrap());
        return Err (OptionError::IncompatibleOptions (err_msg))
    }

    let count = count_arg.is_some();
    let unique = unique_arg.is_some();
    let repeated = repeated_arg.is_some();
    Ok (UniqArguments{
        count,
        unique,
        repeated,
    })
}

// The program may return non-zero if it encounters any IO exception.
// The program doesn't have exactly the same semantics as uniq
// in interactive mode because this program calls read_line. This means if you
// terminate the uniq loop by pressing ctrl-d, you have to press it one more
// time to exit the program. You could manually read the bytes in a loop instead
// to get the same semantics.
fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    let mut output: StdoutLock = stdout().lock();
    let mut error_output = stderr().lock();

    let mut file_args: Vec<&String> = Vec::new();
    let mut dash_args: Vec<&String> = Vec::new();

    let mut stdin_flag = false;
    for arg in args[1..].iter () {
        if arg.trim() == "-" {
            stdin_flag = true;
        } else if arg.starts_with("-") {
            dash_args.push(arg);
        } else {
            file_args.push (arg);
        }
    }

    if file_args.is_empty() {
        stdin_flag = true;
    }

    let dash_parse_result = parse_dash_args(dash_args);
    let uniq_arguments: UniqArguments = match dash_parse_result {
        Ok(uniq_arg) => uniq_arg,
        Err(option_error) => {
            let err_msg = match option_error {
                OptionError::InvalidOption (arg) => format! ("Invalid option -- '{arg}\n'"),
                OptionError::IncompatibleOptions(msg) => format! ("Incompatible options -- {msg}\n"),
            };
            let _ = error_output.write_all(err_msg.as_bytes());
            return ExitCode::FAILURE;
        }
    };

    if stdin_flag {
        // Take input from stdin instead of a file.
        let mut input: StdinLock = stdin().lock();

        return match uniq(&mut input, &mut output, &uniq_arguments) {
            Ok(_) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        }
    }

    let mut had_error: bool = false;
    for arg in file_args.iter() {
        let exit_code = file_uniq(arg, &mut output, &mut error_output, &uniq_arguments);
        had_error |= exit_code  == ExitCode::FAILURE;
    }
    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn file_uniq(
    filename: &String,
    output: &mut impl Write,
    error_output: &mut impl Write,
    uniq_arguments: &UniqArguments
) -> ExitCode {
    let f_result = File::open(filename);

    let file: File = match f_result {
        Ok(file) => file,
        Err(err) => {
            let _ = writeln!(
                error_output,
                "Could not open file {filename}. Reason: '{err}'"
            );
            return ExitCode::FAILURE;
        }
    };

    let mut file_reader = BufReader::new(file);

    let uniq_result = uniq(&mut file_reader, output, uniq_arguments);

    if let Err(err) = uniq_result {
        let _ = writeln!(error_output, "Error for file {filename}. Reason: '{err}'");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn on_streak_start(args: &UniqArguments, line: &String,
                   output: &mut impl Write) -> io::Result<()> {
    if args.unique || args.repeated {
        // can't tell whether this line will be unique or repeated at streak start.
        return Ok(())
    }

    output.write_all(line.as_bytes())?;
    output.flush()?;
    Ok (())
}

fn on_streak_end(args: &UniqArguments, streak_length: usize,
                 line: &String, output: &mut impl Write) -> io::Result<()> {
    if args.unique && streak_length > 1 { // not unique
        return Ok (())
    }
    if args.repeated && streak_length < 2 { // not repeated
        return Ok (())
    }
    if !args.unique && !args.repeated {
        // default uniq eagerly prints unique lines, so nothing to print here.
        return Ok (())
    }
    if args.count {
        output.write_fmt(format_args!("\t{} ", streak_length))?;
    }
    output.write_all(line.as_bytes())?;
    output.flush()?;
    Ok(())
}

fn uniq(input: &mut impl BufRead, output: &mut impl Write,
        args: &UniqArguments) -> Result<(), io::Error> {
    let mut prev_line = String::new();
    let mut line = String::new();
    let mut prev_exists = false;
    let mut streak_length: usize = 0;
    loop {
        line.clear();
        // ? returns io-exception if it happens
        let line_length = input.read_line(&mut line)?;
        if line_length == 0 {
            // check if active streak: guard against if file starts with EOF
            if streak_length > 0 { // print the previous line since this one is empty
                on_streak_end(args, streak_length, &prev_line, output)?;
            }
            break; // EOF;
        }

        // don't print until we know the final streak length
        if prev_exists {
            if prev_line == line { // extend the streak
                streak_length += 1;
            } else { // end a streak (print the previous line)
                on_streak_end(args, streak_length, &prev_line, output)?;
                streak_length = 1;
                on_streak_start(args, &line, output)?;
            }
        } else {
            streak_length = 1;
            on_streak_start(args, &line, output)?;
        }

        std::mem::swap(&mut prev_line, &mut line); // swap without borrow checker issues
        prev_exists = true;
    }
    Ok(())
}
