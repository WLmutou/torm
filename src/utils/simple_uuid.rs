use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU32, Ordering};

/// 简单的 UUID v4 实现
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimpleUuid {
    data: [u8; 16],
}

impl SimpleUuid {
    pub fn new_v4() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        
        let mut data = [0u8; 16];
        
        // Get random bytes
        let rand_bytes = Self::get_random_bytes();
        
        // Version 4 UUID format
        // time_hi_and_version (bits 4-7): version 4
        data[0..4].copy_from_slice(&rand_bytes[0..4]);
        data[6] = (rand_bytes[6] & 0x0F) | 0x40; // Version 4
        data[7] = (rand_bytes[7] & 0x3F) | 0x80; // Variant 1
        
        // Use counter for uniqueness
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        data[8..12].copy_from_slice(&counter.to_be_bytes());
        
        // Add timestamp for more uniqueness
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        
        data[12..16].copy_from_slice(&timestamp.to_be_bytes()[4..8]);
        
        Self { data }
    }

    fn get_random_bytes() -> [u8; 16] {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        
        timestamp.hash(&mut hasher);
        std::thread::current().id().hash(&mut hasher);
        
        let mut bytes = [0u8; 16];
        let hash = hasher.finish();
        bytes[0..8].copy_from_slice(&hash.to_be_bytes());
        
        // Add more entropy
        let mut extra_hasher = DefaultHasher::new();
        let stack_var = &bytes as *const _ as usize;
        stack_var.hash(&mut extra_hasher);
        
        let extra_hash = extra_hasher.finish();
        bytes[8..16].copy_from_slice(&extra_hash.to_be_bytes());
        
        bytes
    }

    pub fn nil() -> Self {
        Self { data: [0u8; 16] }
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { data: bytes }
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.data
    }

    pub fn to_string(&self) -> String {
        format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.data[0], self.data[1], self.data[2], self.data[3],
            self.data[4], self.data[5],
            self.data[6], self.data[7],
            self.data[8], self.data[9],
            self.data[10], self.data[11], self.data[12], self.data[13], self.data[14], self.data[15]
        )
    }

    pub fn is_nil(&self) -> bool {
        self.data == [0u8; 16]
    }

    pub fn version(&self) -> u8 {
        (self.data[6] & 0xF0) >> 4
    }

    pub fn variant(&self) -> u8 {
        self.data[8] & 0xC0
    }
}

impl Default for SimpleUuid {
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for SimpleUuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl std::str::FromStr for SimpleUuid {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; 16];
        let chars: Vec<char> = s.chars().collect();
        
        if chars.len() != 36 {
            return Err("Invalid UUID string length".to_string());
        }

        // Validate format
        if chars[8] != '-' || chars[13] != '-' || chars[18] != '-' || chars[23] != '-' {
            return Err("Invalid UUID format".to_string());
        }

        let hex_str: String = chars.iter()
            .filter(|c| **c != '-')
            .collect();

        for (i, byte) in bytes.iter_mut().enumerate() {
            let pos = i * 2;
            if pos + 1 >= hex_str.len() {
                return Err("Invalid UUID hex string".to_string());
            }
            
            *byte = u8::from_str_radix(&hex_str[pos..pos+2], 16)
                .map_err(|_| "Invalid hex character")?;
        }

        Ok(Self { data: bytes })
    }
}

impl serde::Serialize for SimpleUuid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for SimpleUuid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// 简单的 ID 生成器
pub struct IdGenerator {
    prefix: Option<String>,
    use_uuid: bool,
}

impl IdGenerator {
    pub fn new() -> Self {
        Self {
            prefix: None,
            use_uuid: true,
        }
    }

    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    pub fn with_simple_id(mut self) -> Self {
        self.use_uuid = false;
        self
    }

    pub fn generate(&self) -> String {
        if self.use_uuid {
            let uuid = SimpleUuid::new_v4();
            match &self.prefix {
                Some(prefix) => format!("{}{}", prefix, uuid),
                None => uuid.to_string(),
            }
        } else {
            self.generate_simple_id()
        }
    }

    fn generate_simple_id(&self) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        
        let random = (timestamp % 10000) as u32;
        
        match &self.prefix {
            Some(prefix) => format!("{}-{}-{}", prefix, timestamp, random),
            None => format!("{}-{}", timestamp, random),
        }
    }
}

impl Default for IdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_creation() {
        let uuid = SimpleUuid::new_v4();
        assert!(!uuid.is_nil());
        assert_eq!(uuid.version(), 4);
    }

    #[test]
    fn test_uuid_nil() {
        let uuid = SimpleUuid::nil();
        assert!(uuid.is_nil());
    }

    #[test]
    fn test_uuid_to_string() {
        let uuid = SimpleUuid::new_v4();
        let s = uuid.to_string();
        assert_eq!(s.len(), 36);
        assert!(s.contains('-'));
    }

    #[test]
    fn test_uuid_from_string() {
        let uuid1 = SimpleUuid::new_v4();
        let s = uuid1.to_string();
        let uuid2: SimpleUuid = s.parse().unwrap();
        assert_eq!(uuid1, uuid2);
    }

    #[test]
    fn test_uuid_uniqueness() {
        let uuid1 = SimpleUuid::new_v4();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let uuid2 = SimpleUuid::new_v4();
        assert_ne!(uuid1, uuid2);
    }

    #[test]
    fn test_uuid_serialization() {
        let uuid = SimpleUuid::new_v4();
        let json = serde_json::to_string(&uuid).unwrap();
        let deserialized: SimpleUuid = serde_json::from_str(&json).unwrap();
        assert_eq!(uuid, deserialized);
    }

    #[test]
    fn test_id_generator() {
        let generator = IdGenerator::new();
        let id1 = generator.generate();
        let id2 = generator.generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_id_generator_with_prefix() {
        let generator = IdGenerator::new().with_prefix("user_");
        let id = generator.generate();
        assert!(id.starts_with("user_"));
    }

    #[test]
    fn test_id_generator_simple() {
        let generator = IdGenerator::new().with_simple_id();
        let id = generator.generate();
        assert!(!id.starts_with("user_"));
    }

    #[test]
    fn test_uuid_bytes_conversion() {
        let uuid = SimpleUuid::new_v4();
        let bytes = *uuid.as_bytes();
        let uuid2 = SimpleUuid::from_bytes(bytes);
        assert_eq!(uuid, uuid2);
    }

    #[test]
    fn test_invalid_uuid_string() {
        let result: Result<SimpleUuid, _> = "invalid-uuid".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_uuid_display() {
        let uuid = SimpleUuid::new_v4();
        let display = format!("{}", uuid);
        assert_eq!(display, uuid.to_string());
    }
}
