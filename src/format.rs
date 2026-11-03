pub fn markdown(name: &str, url: &str, is_image: bool) -> String {
    let title = name.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('\r', " ")
        .replace('\n', " ");
    // Angle brackets protect URLs containing spaces or parentheses in CommonMark.
    let safe_url = url.replace('>', "%3E").replace('<', "%3C");
    if is_image { format!("![{title}](<{safe_url}>)") }
    else { format!("[{title}](<{safe_url}>)") }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_markdown_is_escaped() {
        assert_eq!(markdown("a]b.png", "https://example.com/a(1).png", true),
            "![a\\]b.png](<https://example.com/a(1).png>)");
    }
    #[test]
    fn attachment_markdown() {
        assert_eq!(markdown("file.pdf", "https://example.com/file", false),
            "[file.pdf](<https://example.com/file>)");
    }
}
