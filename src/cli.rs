use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CliArgs {
    /// Number of times to greet
    #[arg(short, long, default_value_t = 15f32)]
    pub intensity: f32,

    #[arg(short, long)]
    pub display: Option<String>,
}