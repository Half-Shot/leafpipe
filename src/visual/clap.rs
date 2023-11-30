use clap::{arg, Command};

pub fn set_flags() -> Command<'static> {
    let app = Command::new("pxlha")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about("Match RGB iot lighting with wayland compositor output.")
        .arg(
            arg!(-d - -debug)
                .required(false)
                .takes_value(false)
                .help("Enable debug mode"),
        )
        .arg(
            arg!(-l - -listoutputs)
                .required(false)
                .takes_value(false)
                .help("List all valid outputs"),
        )
        .arg(
            arg!(-o --output <OUTPUT>)
                .required(false)
                .takes_value(true)
                .help("Choose a particular output to use"),
        );
    app
}
