use std::env;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, StdinLock, StdoutLock, Write, stderr, stdin, stdout};
use std::process::ExitCode;

// The program may return non-zero if it encounters any IO exception.
fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();

    let argc = args.len();

    let mut output: StdoutLock = stdout().lock();
    let mut error_output = stderr().lock();

    let mut file_args: Vec<&String> = Vec::new();
    let mut dash_args: Vec<&String> = Vec::new();

    for arg in args[1..].iter () {
        if (arg.starts_with("-")) {
            dash_args.push(arg);
        } else {
            file_args.push (arg);
        }
    }
    // TODO: use dash args like uniq does

    if argc > 1 {
        let mut had_error: bool = false;
        for arg in file_args.iter() {
            had_error = file_uniq(arg, &mut output, &mut error_output) == ExitCode::FAILURE;
        }
        return if had_error {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }
    // basic uniq implementation
    let mut input: StdinLock = stdin().lock();

    match uniq(&mut input, &mut output) {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn file_uniq(
    filename: &String,
    output: &mut impl Write,
    error_output: &mut impl Write,
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

    let uniq_result = uniq(&mut file_reader, output);

    if let Err(err) = uniq_result {
        let _ = writeln!(error_output, "Error for file {filename}. Reason: '{err}'");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn uniq(input: &mut impl BufRead, output: &mut impl Write) -> Result<(), io::Error> {
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
            output.write_all(line.as_bytes())?;
            output.flush()?;
        }

        std::mem::swap(&mut prev_line, &mut line); // swap without borrow checker issues
        prev_exists = true;
    }
    Ok(())
}
