use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use chrono::prelude::*;

#[allow(unused)]
mod kvm;
mod util;
use util::*;

fn main() -> Result<()> {
    /*
     * Determine the current system boot time.
     */
    let k = kvm::Kvm::new()?;
    let btp = k.locate("boot_time")?;
    let bt: u64 = k.read_usize(btp)?.try_into().unwrap();
    let bt_utc = Utc.timestamp_opt(bt.try_into().unwrap(), 0).unwrap();

    println!("boot_time = {bt} ({bt_utc})");

    /*
     * What is the current UNIX time as perceived by the host before
     * corrections?
     */
    let pre_snap = Instant::now();
    let pre_now =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let uptime = (pre_now as i64) - (bt as i64);

    println!("pre_now = {pre_now} (uptime {uptime} seconds)");

    /*
     * Run chrony to fix the clock.
     */
    let res =
        Command::new("/usr/sbin/chronyd").env_clear().arg("-q").output()?;

    if !res.status.success() {
        bail!("chronyd -q failed: {}", res.info());
    }

    /*
     * The clock has now changed!  Find out the current time again.  We use the
     * delta between readings to adjust for the length of time it took for and
     * NTP update.
     */
    let post_snap = Instant::now();
    let post_now =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let fudge = post_snap.checked_duration_since(pre_snap).unwrap().as_secs();
    println!("fudge = {fudge}");

    let shift = (post_now as i64) - (pre_now as i64) - (fudge as i64);
    if shift < 0 {
        println!("WARNING: wall time went backwards!");
    }

    println!("post_now = {post_now} (shifted {shift} seconds)");

    let abt = (bt as i64).checked_add(shift).unwrap();
    println!("boot_time: {bt} -> {abt}");

    /*
     * Adjust boot time to reflect the correction:
     */
    k.write_usize(btp, abt.try_into().unwrap())?;

    for f in ["/var/adm/utmpx", "/var/adm/wtmpx"] {
        let res = Command::new("/usr/platform/oxide/bin/tmpx")
            .env_clear()
            .arg(abt.to_string())
            .arg(f)
            .output()?;

        if !res.status.success() {
            bail!("failed to update {f}: {}", res.info());
        }
    }

    Ok(())
}
