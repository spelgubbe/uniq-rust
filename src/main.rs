use std::env;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, StdinLock, StdoutLock, Write, stderr, stdin, stdout};
use std::process::ExitCode;

struct UniqArguments {
    // argument -c to uniq. Write the count of each streak.
    count_streaks: bool,
    // argument -u to uniq. Renamed here because it's not unique.
    only_streaks_of_one: bool,
    // argument -d / --repeated.
    only_repeated: bool
}

struct UnmatchedArgument {
    arg: String
}

fn parse_dash_args (args: Vec<&String>) -> Result<UniqArguments, UnmatchedArgument> {
    // parse -u / --unique or -d / --repeated into something structured...
    let mut count_streaks = false;
    let mut only_streaks_of_one = false;
    let mut only_repeated = false;
    for arg in args {
        let trimmed_arg = arg.trim();
        match trimmed_arg {
            "-u" | "--unique" => only_streaks_of_one = true,
            "-d" | "--repeated" => only_repeated = true,
            "-c" | "--count" => count_streaks = true,
            _ => {
                // unexpected to have an unmatched argument
                return Err (UnmatchedArgument {arg: trimmed_arg.to_string()})
            }
        }
    }
    Ok (UniqArguments{
        count_streaks,
        only_streaks_of_one,
        only_repeated,
    })
}

// The program may return non-zero if it encounters any IO exception.
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
        Err(UnmatchedArgument {arg}) => {
            let _ = writeln!(
                error_output,
                "Invalid option -- '{arg}'"
            );
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

fn write_line (print_count: bool, streak_length: usize,
               line: &String, output: &mut impl Write) -> io::Result<()> {
    if print_count {
        output.write_fmt(format_args!("\t{} ", streak_length))?;
    }
    output.write_all(line.as_bytes())?;
    output.flush()?;
    Ok (())
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

fn uniq(input: &mut impl BufRead, output: &mut impl Write,
               args: &UniqArguments) -> Result<(), io::Error> {

    if args.only_repeated {
        uniq_repeated(input, output, args)
    } else if args.only_streaks_of_one {
        uniq_unique(input, output, args)
    } else {
        // TODO: unclear what to do to options that can not be combined.
        //  printing nothing is one option...
        Ok (())
    }
}

fn uniq_unique(input: &mut impl BufRead, output: &mut impl Write,
        args: &UniqArguments) -> Result<(), io::Error> {
    // basic uniq implementation
    let mut prev_line = String::new();
    let mut line = String::new();
    let mut prev_exists = false;
    loop {
        line.clear();
        // ? returns io-exception if it happens
        let line_length = input.read_line(&mut line)?;
        if line_length == 0 {
            break; // EOF;
        }

        if !prev_exists || prev_line != line {
            write_line(args.count_streaks, 1, &line, output)?;
        }

        std::mem::swap(&mut prev_line, &mut line); // swap without borrow checker issues
        prev_exists = true;
    }
    Ok(())
}

fn uniq_repeated(input: &mut impl BufRead, output: &mut impl Write,
        args: &UniqArguments) -> Result<(), io::Error> {
    // basic uniq implementation
    let mut prev_line = String::new();
    let mut line = String::new();
    let mut prev_exists = false;
    let mut streak_length: usize = 0;
    loop {
        line.clear();
        // ? returns io-exception if it happens
        let line_length = input.read_line(&mut line)?;
        if line_length == 0 {
            if streak_length > 0 {
                write_line(args.count_streaks, streak_length, &line, output)?;
            }
            break; // EOF;
        }

        if !prev_exists || prev_line == line {
            streak_length += 1;
        }
        if prev_exists && prev_line != line {
            // ends a streak
            write_line(args.count_streaks, streak_length, &line, output)?;
            streak_length = 1;
        }

        std::mem::swap(&mut prev_line, &mut line); // swap without borrow checker issues
        prev_exists = true;
    }
    Ok(())
}
