//! `airth release`: what the release workflow (.github/workflows/release.yml)
//! uses to publish Airth: make the app bundle, sign the update, and the
//! one-time key generation. See docs/RELEASING.md.

use crate::update;
use anyhow::{Context, Result, bail};
use aws_lc_rs::signature::{Ed25519KeyPair, KeyPair};
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ReleaseCmd {
    /// Make Airth.app around a binary (unsigned)
    Bundle {
        /// The binary to put in the app (e.g. a universal one made with lipo)
        #[arg(long)]
        binary: PathBuf,
        /// Where to write the app, e.g. dist/Airth.app
        #[arg(long)]
        out: PathBuf,
    },
    /// Create the update signing key (once): the private key goes to a file,
    /// the public key is printed (it belongs in src/update.rs)
    Keygen {
        #[arg(long)]
        out: PathBuf,
    },
    /// Sign a release archive and write its manifest (latest.json). The
    /// private key comes from AIRTH_UPDATE_KEY.
    Sign {
        /// The app archive (a zip of Airth.app)
        archive: PathBuf,
        #[arg(long)]
        version: String,
        /// Where the archive will be downloaded from
        #[arg(long)]
        url: String,
        /// A file with the release notes, one change per line
        #[arg(long)]
        notes: Option<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
}

pub fn run(command: ReleaseCmd) -> Result<bool> {
    match command {
        ReleaseCmd::Bundle { binary, out } => {
            if out.exists() {
                bail!("{} already exists", out.display());
            }
            crate::service::write_bundle(&out, &binary, crate::service::RELEASE_IDENTIFIER)?;
            eprintln!("made {}", out.display());
        }
        ReleaseCmd::Keygen { out } => {
            if out.exists() {
                bail!("{} already exists: keep using that key", out.display());
            }
            let rng = aws_lc_rs::rand::SystemRandom::new();
            let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
                .map_err(|_| anyhow::anyhow!("cannot generate a key"))?;
            let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
                .map_err(|_| anyhow::anyhow!("cannot read the new key"))?;
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            crate::realm::write_private(&out, &update::to_hex(pkcs8.as_ref()))?;
            eprintln!("private key → {} (keep it secret)", out.display());
            println!("{}", update::to_hex(pair.public_key().as_ref()));
        }
        ReleaseCmd::Sign {
            archive,
            version,
            url,
            notes,
            out,
        } => {
            let key = std::env::var("AIRTH_UPDATE_KEY").context("AIRTH_UPDATE_KEY is not set")?;
            let pkcs8 = update::from_hex(&key).context("AIRTH_UPDATE_KEY is not valid hex")?;
            let pair = Ed25519KeyPair::from_pkcs8(&pkcs8)
                .map_err(|_| anyhow::anyhow!("AIRTH_UPDATE_KEY is not an Ed25519 key"))?;
            let bytes = std::fs::read(&archive)
                .with_context(|| format!("cannot read {}", archive.display()))?;
            let sha256 = update::sha256_hex(&bytes);
            let signature = update::to_hex(
                pair.sign(update::signed_message(&version, &sha256).as_bytes())
                    .as_ref(),
            );
            // A key that does not match the one built into Airth would
            // publish an update nobody can install.
            update::verify_signature(&version, &sha256, &signature)
                .context("AIRTH_UPDATE_KEY does not match the public key in src/update.rs")?;
            let notes = match notes {
                Some(path) => std::fs::read_to_string(&path)?
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(String::from)
                    .collect(),
                None => Vec::new(),
            };
            let manifest = update::Manifest {
                version,
                notes,
                url,
                sha256,
                signature,
            };
            std::fs::write(&out, serde_json::to_string_pretty(&manifest)? + "\n")?;
            eprintln!("signed {} → {}", archive.display(), out.display());
        }
    }
    Ok(true)
}
