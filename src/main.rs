use std::env;
use std::fs::File;
use std::io;
use std::io::{
    BufRead, BufReader, BufWriter, Read, StdoutLock, Write, stderr, stdin, stdout,
};
use std::path::PathBuf;
use std::process::ExitCode;

struct UniqOptions {
    // argument -c to uniq. Write the count of each streak.
    count: bool,
    // argument -u to uniq. Renamed here because it's not unique.
    unique: bool,
    // argument -d / --repeated.
    repeated: bool,
}

struct InputOutputOptions {
    input_file: Option<PathBuf>,
    output_file: Option<PathBuf>,
}

enum OptionError {
    InvalidOption(String),
    IncompatibleOptions(String),
    ExtraOperand(String),
}

fn parse_file_args(args: &[String]) -> Result<InputOutputOptions, OptionError> {
    let extra_operand = args.get(2);
    if let Some(extra_operand_str) = extra_operand {
        // Guard against too many arguments.
        return Err(OptionError::ExtraOperand(extra_operand_str.to_string()));
    }
    let input_file_arg = args.first();
    let output_file_arg = args.get(1);

    let input_file = input_file_arg.map(PathBuf::from);
    let output_file = output_file_arg.map(PathBuf::from);
    Ok(InputOutputOptions {
        input_file,
        output_file,
    })
}

fn parse_dash_args(args: Vec<&String>) -> Result<UniqOptions, OptionError> {
    // parse -u / --unique or -d / --repeated into something structured...
    let mut count_arg: Option<&str> = None;
    let mut repeated_arg: Option<&str> = None;
    let mut unique_arg: Option<&str> = None;
    for arg in args {
        let trimmed_arg = arg.trim();
        match trimmed_arg {
            "-u" | "--unique" => {
                if unique_arg.is_none() {
                    unique_arg = Some(trimmed_arg)
                }
            }
            "-d" | "--repeated" => {
                if repeated_arg.is_none() {
                    repeated_arg = Some(trimmed_arg)
                }
            }
            "-c" | "--count" => {
                if count_arg.is_none() {
                    count_arg = Some(trimmed_arg)
                }
            }
            _ => {
                // unexpected to have an unmatched argument
                return Err(OptionError::InvalidOption(trimmed_arg.to_string()));
            }
        }
    }
    // validation to prevent combination of -u and -d
    if unique_arg.is_some() && repeated_arg.is_some() {
        let mut err_msg: String = String::new();
        err_msg.push_str(unique_arg.unwrap());
        err_msg.push_str(" and ");
        err_msg.push_str(repeated_arg.unwrap());
        return Err(OptionError::IncompatibleOptions(err_msg));
    }

    let count = count_arg.is_some();
    let unique = unique_arg.is_some();
    let repeated = repeated_arg.is_some();
    Ok(UniqOptions {
        count,
        unique,
        repeated,
    })
}

fn write_options_error(err: OptionError, error_output: &mut impl Write) -> Result<(), io::Error> {
    let err_msg = match err {
        OptionError::InvalidOption(arg) => format!("Invalid option -- '{arg}\n'"),
        OptionError::IncompatibleOptions(msg) => format!("Incompatible options -- {msg}\n"),
        OptionError::ExtraOperand(arg) => format!("Extra operand -- '{arg}'"),
    };
    error_output.write_all(err_msg.as_bytes())?;
    Ok(())
}

// The program may return non-zero if it encounters any IO exception.
// The program doesn't have exactly the same semantics as uniq
// in interactive mode because this program calls read_line. This means if you
// terminate the uniq loop by pressing ctrl-d, you have to press it one more
// time to exit the program. You could manually read the bytes in a loop instead
// to get the same semantics.
fn main() -> ExitCode {
    // Using env::args_os would lead to being able to read UTF-8 file names.
    // This code probably panics in case of a file name that isn't valid UTF-8
    let args: Vec<String> = env::args().collect();

    let mut output: StdoutLock = stdout().lock();
    let mut error_output = stderr().lock();

    let mut file_args: Vec<String> = Vec::new();
    let mut dash_args: Vec<&String> = Vec::new();

    for arg in args[1..].iter() {
        if arg.trim() == "-" {
            // skip the dash since it means stdin...
        } else if arg.starts_with("-") {
            dash_args.push(arg);
        } else {
            file_args.push(arg.to_string());
        }
    }

    let dash_parse_result = parse_dash_args(dash_args);

    let uniq_arguments: UniqOptions = match dash_parse_result {
        Ok(uniq_arg) => uniq_arg,
        Err(option_error) => {
            let _ = write_options_error(option_error, &mut output);
            return ExitCode::FAILURE;
        }
    };

    let io_parse_result = parse_file_args(file_args.as_slice());
    let io_options: InputOutputOptions = match io_parse_result {
        Ok(io_options) => io_options,
        Err(option_error) => {
            let _ = write_options_error(option_error, &mut output);
            return ExitCode::FAILURE;
        }
    };

    uniq_wrapper(io_options.input_file, io_options.output_file,
                              &mut error_output, &uniq_arguments)
}

fn uniq_wrapper(
    input_path: Option<PathBuf>,
    output_path: Option<PathBuf>,
    error_output: &mut impl Write,
    uniq_arguments: &UniqOptions,
) -> ExitCode {
    let mut file_reader: Option<BufReader<File>> = None;
    let mut file_writer: Option<BufWriter<File>> = None;

    if let Some(input_path) = input_path {
        let f_result = File::open(&input_path);

        let input_file: File = match f_result {
            Ok(file) => file,
            Err(err) => {
                let input_path_str = input_path.to_str().unwrap_or("<unknown>");
                let _ = writeln!(
                    error_output,
                    "Could not open file {input_path_str}. Reason: '{err}'"
                );
                return ExitCode::FAILURE;
            }
        };
        file_reader = Some(BufReader::new(input_file));
    }

    if let Some(output_path) = output_path {
        let f_result = File::create(&output_path);
        let output_file: File = match f_result {
            Ok(file) => file,
            Err(err) => {
                let output_path_str = output_path.to_str().unwrap_or("<unknown>");
                let _ = writeln!(
                    error_output,
                    "Could not create/write file {output_path_str}. Reason: '{err}'"
                );
                return ExitCode::FAILURE;
            }
        };
        file_writer = Some(BufWriter::new(output_file));
    }

    let mut in_reader: Box<dyn BufRead> = match file_reader {
        Some(reader) => Box::new(reader),
        None => Box::new(stdin().lock()),
    };
    let mut out_writer: Box<dyn Write> = match file_writer {
        Some(writer) => Box::new(writer),
        None => Box::new(stdout().lock()),
    };

    let uniq_result: Result<(), io::Error> =
        uniq(in_reader.by_ref(), out_writer.by_ref(), uniq_arguments);
    if let Err(err) = uniq_result {
        let _ = writeln!(error_output, "Error: '{err}'");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn on_streak_start(args: &UniqOptions, line: &String, output: &mut impl Write) -> io::Result<()> {
    if args.unique || args.repeated {
        // can't tell whether this line will be unique or repeated at streak start.
        // so no printing.
        return Ok(());
    }

    output.write_all(line.as_bytes())?;
    output.flush()?;
    Ok(())
}

fn on_streak_end(
    args: &UniqOptions,
    streak_length: usize,
    line: &String,
    output: &mut impl Write,
) -> io::Result<()> {
    if args.unique && streak_length > 1 {
        // not unique
        return Ok(());
    }
    if args.repeated && streak_length < 2 {
        // not repeated
        return Ok(());
    }
    if !args.unique && !args.repeated {
        // default uniq eagerly prints unique lines, so nothing to print here.
        return Ok(());
    }
    if args.count {
        // using a tab character to indent in the terminal here, probably not standard.
        output.write_fmt(format_args!("\t{} ", streak_length))?;
    }
    output.write_all(line.as_bytes())?;
    output.flush()?;
    Ok(())
}

fn uniq(
    input: &mut impl BufRead,
    output: &mut impl Write,
    args: &UniqOptions,
) -> Result<(), io::Error> {
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
            if streak_length > 0 {
                // print the previous line since this one is empty
                on_streak_end(args, streak_length, &prev_line, output)?;
            }
            break; // EOF;
        }

        // don't print until we know the final streak length
        if prev_exists {
            if prev_line == line {
                // extend the streak
                streak_length += 1;
            } else {
                // end a streak (print the previous line)
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
