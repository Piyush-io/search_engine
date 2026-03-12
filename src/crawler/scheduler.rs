use std::{
    collections::{HashMap, HashSet, VecDeque, BinaryHeap},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Notify};
use tokio::time::sleep;
use crate::crawler::types::UrlTask;
use crate::crawler::policy;

#[derive(Default)]
struct HostQueue {
    queued: BinaryHeap<UrlTask>,
    queued_urls: HashSet<String>,
    inflight: bool,
    ready_enqueued: bool,
    timer_armed: bool,
    next_allowed_at: Option<Instant>,
}

#[derive(Default)]
struct SchedulerState {
    hosts: HashMap<String, HostQueue>,
    ready_hosts: VecDeque<String>,
    pending: usize,
    inflight: usize,
    closed: bool,
}

pub struct CrawlScheduler {
    inner: Mutex<SchedulerState>,
    notify: Notify,
    default_rate_limit_ms: u64,
}

impl CrawlScheduler {
    pub fn new(default_rate_limit_ms: u64) -> Self {
        Self {
            inner: Mutex::new(SchedulerState::default()),
            notify: Notify::new(),
            default_rate_limit_ms,
        }
    }

    pub async fn push_task(self: &Arc<Self>, task: UrlTask) -> bool {
        let mut inner = self.inner.lock().await;
        if inner.closed {
            return false;
        }

        let host = task.host.clone();
        let mut should_enqueue_ready = false;
        let mut reschedule_delay = None;
        {
            let host_state = inner.hosts.entry(host.clone()).or_default();
            if !host_state.queued_urls.insert(task.url.clone()) {
                return false;
            }

            host_state.queued.push(task);
            if !host_state.inflight {
                if host_state
                    .next_allowed_at
                    .is_none_or(|deadline| deadline <= Instant::now())
                {
                    if !host_state.ready_enqueued {
                        host_state.ready_enqueued = true;
                        should_enqueue_ready = true;
                    }
                } else if !host_state.timer_armed {
                    host_state.timer_armed = true;
                    reschedule_delay = host_state
                        .next_allowed_at
                        .map(|deadline| deadline.saturating_duration_since(Instant::now()));
                }
            }
        }

        inner.pending += 1;
        if should_enqueue_ready {
            inner.ready_hosts.push_back(host.clone());
        }

        if let Some(delay) = reschedule_delay {
            let scheduler = Arc::clone(self);
            tokio::spawn(async move {
                sleep(delay).await;
                scheduler.requeue_host(host).await;
            });
        }

        if should_enqueue_ready {
            self.notify.notify_one();
        }

        true
    }

    pub async fn requeue_host(self: &Arc<Self>, host: String) {
        let mut inner = self.inner.lock().await;
        if inner.closed {
            return;
        }

        let mut should_enqueue_ready = false;
        let Some(host_state) = inner.hosts.get_mut(&host) else {
            return;
        };
        host_state.timer_armed = false;

        if !host_state.inflight
            && !host_state.queued.is_empty()
            && !host_state.ready_enqueued
            && host_state
                .next_allowed_at
                .is_none_or(|deadline| deadline <= Instant::now())
        {
            host_state.ready_enqueued = true;
            should_enqueue_ready = true;
        }

        if should_enqueue_ready {
            inner.ready_hosts.push_back(host);
            self.notify.notify_one();
        }
    }

    pub async fn next_task(self: &Arc<Self>) -> Option<UrlTask> {
        loop {
            let mut inner = self.inner.lock().await;
            while let Some(host) = inner.ready_hosts.pop_front() {
                let mut should_reschedule = None::<Duration>;
                let mut out = None;

                if let Some(host_state) = inner.hosts.get_mut(&host) {
                    host_state.ready_enqueued = false;
                    if host_state.inflight || host_state.queued.is_empty() {
                        continue;
                    }

                    if let Some(deadline) = host_state.next_allowed_at {
                        if deadline > Instant::now() {
                            if !host_state.timer_armed {
                                host_state.timer_armed = true;
                                should_reschedule =
                                    Some(deadline.saturating_duration_since(Instant::now()));
                            }
                        } else if let Some(task) = host_state.queued.pop() {
                            host_state.queued_urls.remove(&task.url);
                            host_state.inflight = true;
                            inner.pending = inner.pending.saturating_sub(1);
                            inner.inflight += 1;
                            out = Some(task);
                        }
                    } else if let Some(task) = host_state.queued.pop() {
                        host_state.queued_urls.remove(&task.url);
                        host_state.inflight = true;
                        inner.pending = inner.pending.saturating_sub(1);
                        inner.inflight += 1;
                        out = Some(task);
                    }
                }

                if let Some(delay) = should_reschedule {
                    let scheduler = Arc::clone(self);
                    let host_clone = host.clone();
                    tokio::spawn(async move {
                        sleep(delay).await;
                        scheduler.requeue_host(host_clone).await;
                    });
                }

                if out.is_some() {
                    return out;
                }
            }

            if inner.closed && inner.pending == 0 && inner.inflight == 0 {
                return None;
            }

            drop(inner);
            self.notify.notified().await;
        }
    }

    pub async fn complete_host(self: &Arc<Self>, host: &str) {
        let mut inner = self.inner.lock().await;
        let Some(host_state) = inner.hosts.get_mut(host) else {
            inner.inflight = inner.inflight.saturating_sub(1);
            self.notify.notify_waiters();
            return;
        };

        let mut reschedule_delay = None;
        host_state.inflight = false;
        host_state.next_allowed_at = Some(
            Instant::now()
                + Duration::from_millis(policy::domain_rate_limit_ms(self.default_rate_limit_ms, host)),
        );
        if !host_state.queued.is_empty() && !host_state.timer_armed {
            host_state.timer_armed = true;
            reschedule_delay = host_state
                .next_allowed_at
                .map(|deadline| deadline.saturating_duration_since(Instant::now()));
        }

        inner.inflight = inner.inflight.saturating_sub(1);

        if let Some(delay) = reschedule_delay {
            let scheduler = Arc::clone(self);
            let host_clone = host.to_string();
            tokio::spawn(async move {
                sleep(delay).await;
                scheduler.requeue_host(host_clone).await;
            });
        }

        self.notify.notify_waiters();
    }

    pub async fn stats(&self) -> (usize, usize, usize) {
        let inner = self.inner.lock().await;
        (inner.pending, inner.inflight, inner.hosts.len())
    }

    pub async fn close(&self) {
        let mut inner = self.inner.lock().await;
        inner.closed = true;
        self.notify.notify_waiters();
    }
}
