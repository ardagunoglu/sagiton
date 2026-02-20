use serde::Serialize;

pub const VIEW_CHANNEL: i64 = 1 << 0;
pub const SEND_MESSAGES: i64 = 1 << 1;
pub const MANAGE_CHANNELS: i64 = 1 << 2;
pub const MANAGE_ROLES: i64 = 1 << 3;
pub const KICK_MEMBERS: i64 = 1 << 4;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PermissionBits(pub i64);

impl PermissionBits {
    /// Returns true if all required permission bits are enabled.
    ///
    /// # Parameters
    /// - `required`: Permission bitmask to check.
    pub fn has(self, required: i64) -> bool {
        self.0 & required == required
    }
}
