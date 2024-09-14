use std::{collections::HashMap, hash::Hash, ops::Add, sync::Arc, time::Duration};

use futures::{never::Never, stream::unfold, StreamExt};
use log::error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::UnixStream as TokioUnixStream,
    sync::broadcast::{self},
    time::{sleep_until, Instant},
};
use with_daemon::DaemonControl;

pub struct Worker {
    loads: broadcast::Sender<Arc<HashMap<i32, f32>>>,
}

impl Worker {
    pub async fn new(update_interval: Duration, _: DaemonControl) -> Result<Self, Never> {
        let (loads, _) = broadcast::channel(1);
        let sender = loads.clone();
        tokio::spawn(async move {
            let mut prev = None;
            loop {
                let next_sample_at = Instant::now() + update_interval;
                let current_ticks = get_ticks_since_boot().expect("should know time in ticks");
                let (next, loads) = measure_pid_loads(prev, current_ticks);
                let _ = sender.send(loads.into());
                prev = Some(next);
                sleep_until(next_sample_at).await;
            }
        });
        Ok(Self { loads })
    }

    pub async fn handle_client(self: Arc<Self>, mut stream: TokioUnixStream) {
        let mut loads = self.loads.subscribe();
        let (reader, writer) = stream.split();
        let reader = BufReader::new(reader);
        let mut writer = BufWriter::new(writer);
        let pids: Vec<_> = unfold(reader, |mut reader| async {
            reader.read_i32().await.ok().map(|pid| (pid, reader))
        })
        .collect()
        .await;
        'serving: loop {
            let pid_loads: Vec<_> = {
                let Ok(loads) = loads.recv().await else {
                    continue 'serving;
                };
                pids.iter()
                    .map(|pid| *loads.get(pid).unwrap_or(&f32::NAN))
                    .collect()
            };
            for pid in pid_loads {
                if let Err(e) = writer.write_f32(pid).await {
                    error!("error writing response: {e}");
                    break 'serving;
                }
            }
            if let Err(e) = writer.flush().await {
                error!("error flushing stream: {e}");
                break 'serving;
            }
        }
        if let Err(e) = stream.shutdown().await {
            error!("error shutting down: {e}");
        }
    }
}

/// Perform one measurement of CPU loads for each process tree.
fn measure_pid_loads(prev: Option<Sample>, current_ticks: u64) -> (Sample, HashMap<i32, f32>) {
    // The following words always refer to the following specific concepts:
    // total - total number of ticks used by some process or multiple processes since creation,
    // cumulated - the sum of values of a certain property over a process and all its descendants,
    // recent - one that happened before the last measurement and the current measurement.

    let mut children: HashMap<_, Vec<_>> = HashMap::new();
    let all_procs = procfs::process::all_processes().expect("can't read /proc");
    let dt = current_ticks - prev.as_ref().map(|prev| prev.t).unwrap_or(0);
    let samples = all_procs.filter_map(|prc| {
        let stat = prc.and_then(|prc| prc.stat()).ok()?;
        let sample = PidSample {
            // total time in ticks spent by process in user and kernel since creation
            total_self_ticks: stat.utime + stat.stime,
            // total time in ticks spent by process's children (direct descendants only), that does
            // not include the ones that are still alive (and is not cumulated just yet!)
            cumulated_total_subtree_ticks: stat.cutime + stat.cstime,
        };
        if stat.ppid != 0 {
            children.entry(stat.ppid).or_default().push(stat.pid);
        }
        children.entry(stat.pid).or_default();
        Some((stat.pid, sample))
    });
    let mut samples: HashMap<_, _> = samples.collect();
    let actually_cumulated_total_subtree_ticks = get_cumulated(&children, |id| {
        samples
            .get(&id)
            .expect("samples must contain pid")
            .cumulated_total_subtree_ticks
    });
    for (k, v) in &mut samples {
        // Now, cumulated is actually cumulated; still, this only includes the ticks spent by
        // processes that have already died.
        v.cumulated_total_subtree_ticks = *actually_cumulated_total_subtree_ticks
            .get(k)
            .expect("actually cumulated must contain pid");
    }
    let cur = Sample {
        t: current_ticks,
        pids: samples,
        children,
    };

    // These are the loads of each process (without any descendants included) calculated as a ratio
    // of the number of ticks spent since the last measurement to the total number of ticks
    // elapsed since then.
    // For the first measurement, it's the average loads since boot.
    let self_loads_since_prev: HashMap<_, _> = cur
        .pids
        .iter()
        .map(|(pid, sample)| {
            let prev_sample = prev.as_ref().and_then(|prev| prev.pids.get(pid));
            let self_ticks_since_prev =
                sample.total_self_ticks - prev_sample.map(|p| p.total_self_ticks).unwrap_or(0);
            (*pid, (self_ticks_since_prev as f32 / dt as f32))
        })
        .collect();

    let almost_loads = get_cumulated(&cur.children, |id| {
        *self_loads_since_prev
            .get(&id)
            .expect("itermediate shouldn't miss any value")
    });

    let empty = HashMap::new();
    let prev_children = prev.as_ref().map(|prev| &prev.children).unwrap_or(&empty);
    // The total ticks spent by processes that existed in previous sample and are dead now, but
    // measured only until the previous sample, i.e. excluding any ticks they have spent between
    // the last measurement and the time they died. Cumulated over whole subtrees.
    let prev_cumulated_total_ticks_killed_recently = get_cumulated(prev_children, |id| {
        if cur.pids.contains_key(&id) {
            // we don't care about tasks alive now
            return 0;
        }
        // we'll need to subtract total ticks until previous sample
        prev.as_ref()
            .expect("prev must be some at this point") // otherwise, prev_children would be empty
            .pids
            .get(&id)
            .expect("prev must contain pid") // because id is from prev_children
            .total_self_ticks
    });

    // Now, almost_loads contains loads based on ticks spent by all descendants still alive. But
    // there are also descendants already dead now that have contributed to the total load:
    // 1. those that were spawned and then died between the last sample and now,
    // 2. those that were spawned before the last sample and died between the last sample and now -
    //    their loads were taken into account in the previous measurement, but the ticks they spent
    //    between then and now have not been accounted for.
    let final_loads = almost_loads.into_iter().map(|(pid, self_load)| {
        let cur_total_subtree_ticks = cur
            .pids
            .get(&pid)
            .expect("cur shouldn't miss any values")
            .cumulated_total_subtree_ticks;
        let prev_total_subtree_ticks = prev
            .as_ref()
            .and_then(|prev| prev.pids.get(&pid))
            .map(|s| s.cumulated_total_subtree_ticks)
            .unwrap_or(0);
        // If we subtract the ticks spent by all descendants killed before previous measurement from
        // the time spent by all descendats killed before current measurement, we get the ticks
        // spent by all descendants that were killed exactly between the last measurement and the
        // current one. That is almost what we want, with one exception.
        // 1. spawned recently and already dead - ok,
        // 2. spawned earlier and already dead - they contribute the total ticks, even those spent
        //    before the previous measurement - not ok.
        let ticks_of_recently_killed = (cur_total_subtree_ticks - prev_total_subtree_ticks) as i64;
        let until_prev = *prev_cumulated_total_ticks_killed_recently
            .get(&pid)
            .unwrap_or(&0);
        // And that's our offset described above the loop.
        let offset = ticks_of_recently_killed - until_prev as i64;
        (pid, self_load + offset as f32 / dt as f32)
    });
    let final_loads = final_loads.collect();
    (cur, final_loads)
}

struct Sample {
    /// Time in tics the sample was captured at
    t: u64,
    /// A sample for every discovered process
    pids: HashMap<i32, PidSample>,
    /// The process tree based on the parent-child relationship in form of adjacency lists
    children: HashMap<i32, Vec<i32>>,
}

struct PidSample {
    /// The total time in ticks consumed by the process since its creation.
    total_self_ticks: u64,
    /// The total time in ticks consumed by all process's waited-for descendants (not just
    /// children).
    ///
    /// This only includes processes that are alredy dead at the time the sample is acquired.
    cumulated_total_subtree_ticks: i64,
}

fn get_ticks_since_boot() -> Result<u64, ()> {
    let mut t = libc::tms {
        tms_utime: 0,
        tms_stime: 0,
        tms_cutime: 0,
        tms_cstime: 0,
    };
    let ticks = unsafe { libc::times(&mut t) };
    if ticks < 0 {
        Err(())?
    }
    Ok(ticks as u64)
}

fn get_cumulated<Id, V, F>(children: &HashMap<Id, Vec<Id>>, value: F) -> HashMap<Id, V>
where
    Id: Copy + Eq + Hash,
    V: Copy + Add<V, Output = V> + std::iter::Sum,
    F: Fn(Id) -> V,
{
    let value = &value;
    let mut cumulated_loads = HashMap::new();
    for node in children.keys() {
        cumulate(*node, children, value, &mut cumulated_loads);
    }
    cumulated_loads
}

fn cumulate<Id, V, F>(
    root: Id,
    children: &HashMap<Id, Vec<Id>>,
    value: F,
    cumulated: &mut HashMap<Id, V>,
) -> V
where
    Id: Copy + Eq + Hash,
    V: Copy + Add<V, Output = V> + std::iter::Sum,
    F: Fn(Id) -> V + Clone,
{
    if let Some(c) = cumulated.get(&root) {
        return *c;
    }
    let total = value(root)
        + children
            .get(&root)
            .expect("every id must have children")
            .iter()
            .map(|c| cumulate(*c, children, value.clone(), cumulated))
            .sum();
    cumulated.insert(root, total);
    total
}
