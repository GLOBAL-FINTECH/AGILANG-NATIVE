use anyhow::{bail, Context, Result};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .try_init()
        .ok();

    let command = std::env::args().nth(1).unwrap_or_else(|| "info".to_owned());
    match command.as_str() {
        "info" => print_info(),
        "doctor" => doctor(),
        "smoke" => smoke_test(),
        "--version" | "version" => {
            let packed = agilang_runtime_abi::agi_runtime_abi_version();
            println!(
                "AGILANG Native Runtime {} (ABI {}.{}.{})",
                env!("CARGO_PKG_VERSION"),
                packed >> 16,
                (packed >> 8) & 0xff,
                packed & 0xff
            );
            Ok(())
        }
        other => bail!("unknown command: {other}; expected info, doctor, smoke, or version"),
    }
}

fn print_info() -> Result<()> {
    let info = agilang_runtime_platform::platform_info().context("read platform information")?;
    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}

fn doctor() -> Result<()> {
    print_info()?;
    smoke_test()?;
    println!(
        "live_handles={}",
        agilang_runtime_abi::agi_runtime_live_handles()
    );
    println!("runtime_status=healthy");
    Ok(())
}

fn smoke_test() -> Result<()> {
    agilang_runtime_abi::agi_runtime_reset();
    let number = agilang_runtime_abi::agi_value_int(1990);
    let array = agilang_runtime_abi::agi_value_array();
    let mut next = 0;
    let status = unsafe { agilang_runtime_abi::agi_array_push(array, number, &mut next) };
    if status != 0 {
        bail!("array push failed with status {status}");
    }
    let mut item = 0;
    if unsafe { agilang_runtime_abi::agi_array_get(next, 0, &mut item) } != 0 {
        bail!("array read failed");
    }
    let mut value = 0_i64;
    if unsafe { agilang_runtime_abi::agi_value_read_int(item, &mut value) } != 0 || value != 1990 {
        bail!("ABI integer round trip failed");
    }

    let async_result = agilang_runtime_async::block_on(async {
        let (tx, mut rx) = agilang_runtime_async::bounded_channel(1)?;
        tx.send(42_u64).await?;
        Ok::<_, agilang_runtime_core::AgilangError>(rx.receive().await)
    })??;
    if async_result != Some(42) {
        bail!("async channel failed");
    }

    for handle in [item, next, array, number] {
        if agilang_runtime_abi::agi_handle_release(handle) != 0 {
            bail!("handle release failed");
        }
    }
    if agilang_runtime_abi::agi_runtime_live_handles() != 0 {
        bail!("handle leak detected");
    }
    println!("AGILANG native runtime smoke test passed");
    Ok(())
}
