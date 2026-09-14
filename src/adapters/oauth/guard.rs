//! OAuth 2.1 密码防暴力破解安全守卫 (Guard Pattern)

use crate::oauth::OauthError;
use std::time::{Duration, Instant};

const MAX_FAILURES: usize = 8;
const FAILURE_WINDOW: Duration = Duration::from_secs(300);
const LOCKOUT: Duration = Duration::from_secs(30);

/// 专职防暴力破解守卫
pub struct BruteForceGuard {
    failures: Vec<Instant>,
    lock_until: Option<Instant>,
}

impl BruteForceGuard {
    pub fn new() -> Self {
        Self {
            failures: Vec::new(),
            lock_until: None,
        }
    }

    /// 校验密码结果并更新安全状态机
    pub fn check(&mut self, is_correct: bool) -> Result<(), OauthError> {
        let now = Instant::now();

        // 1. 检查是否仍处于锁定期
        if self.lock_until.is_some_and(|until| until > now) {
            return Err(OauthError::Locked);
        }

        // 2. 清理滑动窗口外的陈旧失败记录
        self.failures
            .retain(|t| now.duration_since(*t) < FAILURE_WINDOW);

        // 3. 密码正确，重置失败计数
        if is_correct {
            self.failures.clear();
            return Ok(());
        }

        // 4. 密码错误，累加失败次数
        self.failures.push(now);
        if self.failures.len() >= MAX_FAILURES {
            self.lock_until = Some(now + LOCKOUT);
            self.failures.clear();
            return Err(OauthError::Locked);
        }

        Err(OauthError::AccessDenied("invalid password".into()))
    }
}
