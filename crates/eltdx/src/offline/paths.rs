use super::{invalid, io_error, Exchange, LocalCode};
use crate::error::Result;
use std::path::{Path, PathBuf};

/// Explicit opt-in discovery. No builder or getter calls this automatically.
pub fn detect_tdx_home() -> Result<Option<PathBuf>> {
    if let Some(value) = std::env::var_os("TDX_HOME").filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(value);
        if let Some(candidate) =
            python_legacy::first_directory([Ok(candidate)], |value| is_directory(value))?
        {
            return Ok(Some(candidate));
        }
    }
    #[cfg(windows)]
    let candidates = [
        PathBuf::from(r"C:\new_jyplug"),
        PathBuf::from(r"C:\new_tdx"),
        PathBuf::from(r"D:\new_jyplug"),
        PathBuf::from(r"D:\new_tdx"),
    ]
    .to_vec();
    #[cfg(not(windows))]
    let candidates = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|base| vec![base.join("new_jyplug"), base.join("new_tdx")])
        .unwrap_or_default();
    python_legacy::first_directory(candidates.into_iter().map(Ok), |value| is_directory(value))
}

pub fn resolve_vipdoc(explicit: Option<&Path>) -> Result<PathBuf> {
    let candidate = match explicit {
        Some(path) => path.to_path_buf(),
        None => detect_tdx_home()?
            .ok_or_else(|| {
                io_error(
                    "detect_home",
                    Path::new("TDX_HOME"),
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "TDX installation was not found",
                    ),
                )
            })?
            .join("vipdoc"),
    };
    if is_directory(&candidate)? {
        return Ok(candidate);
    }
    Err(io_error(
        "resolve_vipdoc",
        &candidate,
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "vipdoc directory was not found",
        ),
    ))
}

fn is_directory(path: &Path) -> Result<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("metadata", path, error)),
    }
}

pub fn market_to_exchange(market: u8) -> Result<Exchange> {
    python_legacy::market_to_exchange_with(|value| Ok::<_, crate::Error>(market == value))?
        .ok_or_else(|| invalid("local vipdoc market must be 0 (SZ) or 1 (SH)"))
}

#[doc(hidden)]
pub mod python_legacy {
    use super::Exchange;
    pub fn first_directory<T, E>(
        candidates: impl IntoIterator<Item = std::result::Result<T, E>>,
        mut is_directory: impl FnMut(&T) -> std::result::Result<bool, E>,
    ) -> std::result::Result<Option<T>, E> {
        for candidate in candidates {
            let candidate = candidate?;
            if is_directory(&candidate)? {
                return Ok(Some(candidate));
            }
        }
        Ok(None)
    }
    pub fn market_to_exchange_with<E>(
        mut is_market: impl FnMut(u8) -> std::result::Result<bool, E>,
    ) -> std::result::Result<Option<Exchange>, E> {
        for (id, exchange) in [(0, Exchange::Shenzhen), (1, Exchange::Shanghai)] {
            if is_market(id)? {
                return Ok(Some(exchange));
            }
        }
        Ok(None)
    }
}

fn find(
    market: u8,
    code: &LocalCode,
    vipdoc: Option<&Path>,
    folder: &str,
    extension: &str,
) -> Result<PathBuf> {
    let exchange = market_to_exchange(market)?.as_str();
    Ok(resolve_vipdoc(vipdoc)?
        .join(exchange)
        .join(folder)
        .join(format!("{exchange}{}.{extension}", code.as_str())))
}
pub fn find_daily(market: u8, code: &LocalCode, vipdoc: Option<&Path>) -> Result<PathBuf> {
    find(market, code, vipdoc, "lday", "day")
}
pub fn find_lc1(market: u8, code: &LocalCode, vipdoc: Option<&Path>) -> Result<PathBuf> {
    find(market, code, vipdoc, "minline", "lc1")
}
pub fn find_lc5(market: u8, code: &LocalCode, vipdoc: Option<&Path>) -> Result<PathBuf> {
    find(market, code, vipdoc, "fzline", "lc5")
}
pub fn find_5(market: u8, code: &LocalCode, vipdoc: Option<&Path>) -> Result<PathBuf> {
    find(market, code, vipdoc, "fzline", "5")
}
