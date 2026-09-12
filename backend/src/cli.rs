use crate::scode::{self, Address};
use std::io::{Read, Write};

pub fn scode(args: &[String], input: impl Read, mut output: impl Write) -> Result<(), String> {
    if args.first().is_some_and(|arg| arg == "--help" || arg == "-h") || args.is_empty() {
        writeln!(
            output,
            "Usage: justlocationd scode <encode|decode|import> [--without-cells] [--without-wifi]\n\
            Reads UTF-8 from stdin and writes to stdout.\n\
            encode: address JSON to a wrapped S code.\n\
            decode: S code to unchanged address JSON.\n\
            import: S code to a new address ID and from=2; preserves attachments by default.\n\
            Attachment flags apply only to import. No simulation state is changed."
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let mode = args[0].as_str();
    if !["encode", "decode", "import"].contains(&mode) {
        return Err("unknown S code command; use scode --help".into());
    }
    let mut cells = true;
    let mut wifi = true;
    for flag in &args[1..] {
        match (mode, flag.as_str()) {
            ("import", "--without-cells") => cells = false,
            ("import", "--without-wifi") => wifi = false,
            _ => return Err(format!("unexpected S code argument: {flag}")),
        }
    }
    let mut text = String::new();
    input.take(scode::MAX_SIZE as u64 + 1).read_to_string(&mut text).map_err(|e| e.to_string())?;
    if text.len() > scode::MAX_SIZE {
        return Err("S code input exceeds 2 MiB".into());
    }
    let result = match mode {
        "encode" => {
            let address: Address = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            scode::encode(&address)?
        }
        "decode" => {
            serde_json::to_string_pretty(&scode::decode(&text)?).map_err(|e| e.to_string())? + "\n"
        }
        "import" => {
            serde_json::to_string_pretty(&scode::import(&text, cells, wifi)?)
                .map_err(|e| e.to_string())?
                + "\n"
        }
        _ => unreachable!(),
    };
    output.write_all(result.as_bytes()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stdin_pipeline_and_errors_have_separate_outputs() {
        let input = br#"{"latitude":1.123456789,"longitude":2,"nearbyCells":[],"nearbyWifis":[]}"#;
        let mut encoded = Vec::new();
        scode(&["encode".into()], input.as_slice(), &mut encoded).unwrap();
        let mut imported = Vec::new();
        scode(&["import".into(), "--without-wifi".into()], encoded.as_slice(), &mut imported)
            .unwrap();
        let address: Address = serde_json::from_slice(&imported).unwrap();
        assert_eq!(address.0["latitude"], 1.123456789);
        assert!(address.0.contains_key("nearbyCells"));
        assert!(!address.0.contains_key("nearbyWifis"));
        let mut output = Vec::new();
        assert!(scode(&["decode".into(), "--without-wifi".into()], &b""[..], &mut output).is_err());
        assert!(output.is_empty());
        assert!(scode(&["decode".into()], &b"invalid"[..], &mut output).is_err());
        assert!(output.is_empty());
    }
}
