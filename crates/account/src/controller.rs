//! 账户能力状态：登录会话、受信任设备与配置同步快照/事件应用。

use std::sync::Arc;

use fluxdown_protocol::{
    AgentEvent, AgentSessionDto, AgentSnapshot, CloudDevice, ServiceEvent, SyncStatusDto,
};

use crate::AccountPort;

pub struct AccountController {
    pub(crate) port: Arc<dyn AccountPort>,
    session: Option<AgentSessionDto>,
    devices: Vec<CloudDevice>,
    sync: SyncStatusDto,
    stale: bool,
}

impl AccountController {
    #[must_use]
    pub fn new(port: Arc<dyn AccountPort>) -> Self {
        Self {
            port,
            session: None,
            devices: Vec::new(),
            sync: SyncStatusDto::default(),
            stale: true,
        }
    }

    pub fn replace_snapshot(&mut self, snapshot: &AgentSnapshot) {
        self.session.clone_from(&snapshot.session);
        self.devices.clone_from(&snapshot.cloud_devices);
        self.sync = snapshot.sync.clone();
        self.stale = false;
    }

    pub fn apply_event(&mut self, event: &ServiceEvent) {
        let ServiceEvent::Agent(event) = event else {
            return;
        };
        match event {
            AgentEvent::SessionChanged(session) => self.session.clone_from(session.as_ref()),
            AgentEvent::CloudDevicesChanged(devices) => self.devices.clone_from(devices),
            AgentEvent::SyncChanged(status) => self.sync = status.clone(),
            _ => {}
        }
    }

    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    #[must_use]
    pub fn session(&self) -> Option<&AgentSessionDto> {
        self.session.as_ref()
    }

    #[must_use]
    pub fn devices(&self) -> &[CloudDevice] {
        &self.devices
    }

    #[must_use]
    pub fn sync_status(&self) -> &SyncStatusDto {
        &self.sync
    }

    #[must_use]
    pub fn is_stale(&self) -> bool {
        self.stale
    }

    /// 供页面渲染函数发起额外命令（设备刷新、登出、同步开关……）。
    #[must_use]
    pub(crate) fn port(&self) -> Arc<dyn AccountPort> {
        self.port.clone()
    }
}
