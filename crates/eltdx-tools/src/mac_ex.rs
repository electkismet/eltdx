use std::fs::File;
use std::io::Read;

use eltdx::mac::handshake::LoginReplyRules;
use eltdx::mac::MacExClientBuilder;
use serde::Deserialize;

const CONFIG_ENV: &str = "ELTDX_MAC_EX_LOGIN_RULES";
const MAX_CONFIG_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyConfig {
    evidence: String,
    accepted_hex: Vec<String>,
    #[serde(default)]
    rejected_hex: Vec<String>,
}

pub fn configure_login(
    builder: MacExClientBuilder,
) -> Result<MacExClientBuilder, Box<dyn std::error::Error>> {
    let path = std::env::var_os(CONFIG_ENV).ok_or(
        "MAC-EX needs verified login reply rules. Set ELTDX_MAC_EX_LOGIN_RULES to a JSON rules file.",
    )?;
    let mut data = Vec::new();
    File::open(path)?
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_CONFIG_BYTES {
        return Err("MAC-EX login rules file exceeds 5 MiB".into());
    }
    let config: ReplyConfig = serde_json::from_slice(&data)?;
    let parse = |values: Vec<String>| -> Result<_, Box<dyn std::error::Error>> {
        values
            .into_iter()
            .map(|value| decode_hex(&value).map(Into::into))
            .collect()
    };
    let rules = LoginReplyRules::new(
        parse(config.accepted_hex)?,
        parse(config.rejected_hex)?,
        config.evidence,
    )?;
    Ok(builder.with_login_reply_rules(rules))
}

fn decode_hex(value: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if !value.is_ascii()
        || !value.len().is_multiple_of(2)
        || value.len() / 2 > usize::from(u16::MAX)
    {
        return Err(
            "MAC-EX login payload must be an even-length hexadecimal string within wire limits"
                .into(),
        );
    }
    (0..value.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).map_err(Into::into))
        .collect()
}
