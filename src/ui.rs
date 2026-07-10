use colored::*;

// Enable ANSI colors on Windows
pub fn init() {
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).ok();
}

const W: usize = 46;

fn line(content: &str, blue: Color) -> String {
    let pad = if content.len() < W { W - content.len() } else { 0 };
    format!("  {} {}{} {}",
        "║".color(blue),
        content,
        " ".repeat(pad),
        "║".color(blue))
}

fn line_colored(visible_len: usize, colored_content: String, blue: Color) -> String {
    let pad = if visible_len < W { W - visible_len } else { 0 };
    format!("  {} {}{} {}",
        "║".color(blue),
        colored_content,
        " ".repeat(pad),
        "║".color(blue))
}

pub fn print_welcome() {
    init();

    let blue = Color::TrueColor { r: 60, g: 120, b: 216 };
    let gray = Color::TrueColor { r: 120, g: 120, b: 120 };
    let red  = Color::TrueColor { r: 220, g: 40, b: 40 };

    let bar_top = format!("  {}{}{}", "╔".color(blue), "═".repeat(W + 2).color(blue), "╗".color(blue));
    let bar_mid = format!("  {}{}{}", "╠".color(blue), "═".repeat(W + 2).color(blue), "╣".color(blue));
    let bar_bot = format!("  {}{}{}", "╚".color(blue), "═".repeat(W + 2).color(blue), "╝".color(blue));
    let blank   = line("", blue);

    println!();
    println!("{}", bar_top);

    // Title
    let title = format!("{}{}", "KAIRO".bold().white(), "DB".color(blue).bold());
    println!("{}", line_colored(7, title, blue));

    let sub = format!("{}", "human-readable databases".color(gray));
    println!("{}", line_colored(24, sub, blue));

    println!("{}", bar_mid);
    println!("{}", blank);

    // Commands header
    let hdr = format!("{}", "COMMANDS".color(gray).bold());
    println!("{}", line_colored(8, hdr, blue));
    println!("{}", blank);

    // Command rows
    let cmds = [
        ("init",   "set up a new project"),
        ("create", "apply a .kairo schema"),
        ("query",  "run a database query"),
        ("read",   "inspect a database file"),
        ("export", "convert db to .kairo"),
        ("tables", "list all tables"),
        ("status", "show project info"),
    ];

    for (cmd, desc) in &cmds {
        let colored = format!("{} {}", format!("{:<8}", cmd).bold().white(), desc.color(gray));
        let visible = format!("{:<8} {}", cmd, desc);
        println!("{}", line_colored(visible.len(), colored, blue));
    }

    println!("{}", blank);
    println!("{}", bar_mid);

    // Footer
    let url = format!("{}", "kairo.infiniware.bid".color(blue));
    println!("{}", line_colored(20, url, blue));

    let brand = format!("{} {}{}", "made by".color(gray), "fut0r".color(red).bold(), "".white().bold());
    println!("{}", line_colored(18, brand, blue));

    println!("{}", bar_bot);
    println!();
}

pub fn print_success(msg: &str) {
    init();
    let blue = Color::TrueColor { r: 60, g: 120, b: 216 };
    println!("  {} {}", "OK".color(blue).bold(), msg);
}

pub fn print_dim(msg: &str) {
    init();
    let gray = Color::TrueColor { r: 120, g: 120, b: 120 };
    println!("  {}", msg.color(gray));
}

pub fn print_header(msg: &str) {
    init();
    let blue = Color::TrueColor { r: 60, g: 120, b: 216 };
    println!();
    println!("  {}", msg.color(blue).bold());
    println!("  {}", "─".repeat(msg.len()).color(blue));
}

pub fn print_row(label: &str, value: &str) {
    init();
    let gray = Color::TrueColor { r: 120, g: 120, b: 120 };
    println!("  {}  {}", label.white().bold(), value.color(gray));
}

#[allow(dead_code)]
pub fn print_error(msg: &str) {
    init();
    let red = Color::TrueColor { r: 220, g: 40, b: 40 };
    eprintln!("  {} {}", "ERROR".color(red).bold(), msg);
}
