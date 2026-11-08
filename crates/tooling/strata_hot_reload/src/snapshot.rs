//! Reactive state snapshots for state-preserving hot reload.

use std::collections::HashMap;
use std::sync::Arc;

/// Dynamic value representation for schema-migratable reactive signals.
#[derive(Debug, Clone)]
pub enum GenerationalFieldValue {
    Int(i64),
    Uint(u64),
    Float(f64),
    Bool(bool),
    Text(String),
    Bytes(Vec<u8>),
    List(Vec<GenerationalFieldValue>),
    Record(HashMap<String, GenerationalFieldValue>),
}

impl PartialEq for GenerationalFieldValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Uint(a), Self::Uint(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Bytes(a), Self::Bytes(b)) => a == b,
            (Self::List(a), Self::List(b)) => a == b,
            (Self::Record(a), Self::Record(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for GenerationalFieldValue {}

impl GenerationalFieldValue {
    /// Encodes field value into binary stream.
    pub fn encode(&self, buf: &mut Vec<u8>) {
        match self {
            Self::Int(val) => {
                buf.push(1);
                buf.extend_from_slice(&val.to_le_bytes());
            }
            Self::Uint(val) => {
                buf.push(2);
                buf.extend_from_slice(&val.to_le_bytes());
            }
            Self::Float(val) => {
                buf.push(3);
                buf.extend_from_slice(&val.to_bits().to_le_bytes());
            }
            Self::Bool(val) => {
                buf.push(4);
                buf.push(if *val { 1 } else { 0 });
            }
            Self::Text(val) => {
                buf.push(5);
                let bytes = val.as_bytes();
                buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(bytes);
            }
            Self::Bytes(val) => {
                buf.push(6);
                buf.extend_from_slice(&(val.len() as u32).to_le_bytes());
                buf.extend_from_slice(val);
            }
            Self::List(val) => {
                buf.push(7);
                buf.extend_from_slice(&(val.len() as u32).to_le_bytes());
                for item in val {
                    item.encode(buf);
                }
            }
            Self::Record(val) => {
                buf.push(8);
                buf.extend_from_slice(&(val.len() as u32).to_le_bytes());
                for (k, v) in val {
                    let kb = k.as_bytes();
                    buf.extend_from_slice(&(kb.len() as u16).to_le_bytes());
                    buf.extend_from_slice(kb);
                    v.encode(buf);
                }
            }
        }
    }

    /// Decodes field value from binary slice.
    pub fn decode(bytes: &[u8], offset: &mut usize) -> Option<Self> {
        if *offset >= bytes.len() {
            return None;
        }
        let tag = bytes[*offset];
        *offset += 1;
        match tag {
            1 => {
                if bytes.len() < *offset + 8 {
                    return None;
                }
                let val = i64::from_le_bytes(bytes[*offset..*offset + 8].try_into().ok()?);
                *offset += 8;
                Some(Self::Int(val))
            }
            2 => {
                if bytes.len() < *offset + 8 {
                    return None;
                }
                let val = u64::from_le_bytes(bytes[*offset..*offset + 8].try_into().ok()?);
                *offset += 8;
                Some(Self::Uint(val))
            }
            3 => {
                if bytes.len() < *offset + 8 {
                    return None;
                }
                let bits = u64::from_le_bytes(bytes[*offset..*offset + 8].try_into().ok()?);
                *offset += 8;
                Some(Self::Float(f64::from_bits(bits)))
            }
            4 => {
                if *offset >= bytes.len() {
                    return None;
                }
                let b = bytes[*offset] != 0;
                *offset += 1;
                Some(Self::Bool(b))
            }
            5 => {
                if bytes.len() < *offset + 4 {
                    return None;
                }
                let len = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
                *offset += 4;
                if bytes.len() < *offset + len {
                    return None;
                }
                let s = String::from_utf8(bytes[*offset..*offset + len].to_vec()).ok()?;
                *offset += len;
                Some(Self::Text(s))
            }
            6 => {
                if bytes.len() < *offset + 4 {
                    return None;
                }
                let len = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
                *offset += 4;
                if bytes.len() < *offset + len {
                    return None;
                }
                let b = bytes[*offset..*offset + len].to_vec();
                *offset += len;
                Some(Self::Bytes(b))
            }
            7 => {
                if bytes.len() < *offset + 4 {
                    return None;
                }
                let count = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
                *offset += 4;
                let mut list = Vec::with_capacity(count);
                for _ in 0..count {
                    list.push(Self::decode(bytes, offset)?);
                }
                Some(Self::List(list))
            }
            8 => {
                if bytes.len() < *offset + 4 {
                    return None;
                }
                let count = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().ok()?) as usize;
                *offset += 4;
                let mut record = HashMap::with_capacity(count);
                for _ in 0..count {
                    if bytes.len() < *offset + 2 {
                        return None;
                    }
                    let klen = u16::from_le_bytes(bytes[*offset..*offset + 2].try_into().ok()?) as usize;
                    *offset += 2;
                    if bytes.len() < *offset + klen {
                        return None;
                    }
                    let k = String::from_utf8(bytes[*offset..*offset + klen].to_vec()).ok()?;
                    *offset += klen;
                    let v = Self::decode(bytes, offset)?;
                    record.insert(k, v);
                }
                Some(Self::Record(record))
            }
            _ => None,
        }
    }
}

/// Migration action for an individual struct field.
#[derive(Clone)]
pub enum FieldAction {
    Rename {
        new_name: String,
    },
    AddDefault {
        default_value: GenerationalFieldValue,
    },
    Remove,
    Convert(Arc<dyn Fn(GenerationalFieldValue) -> GenerationalFieldValue + Send + Sync>),
}

impl std::fmt::Debug for FieldAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rename { new_name } => f.debug_struct("Rename").field("new_name", new_name).finish(),
            Self::AddDefault { default_value } => {
                f.debug_struct("AddDefault").field("default_value", default_value).finish()
            }
            Self::Remove => write!(f, "Remove"),
            Self::Convert(_) => write!(f, "Convert(<closure>)"),
        }
    }
}

/// Schema transformation rule targeting a single field.
#[derive(Debug, Clone)]
pub struct FieldMigration {
    pub name: String,
    pub action: FieldAction,
}

/// Schema migration descriptor transforming signals from one layout generation to the next.
#[derive(Debug, Clone)]
pub struct SignalMigration {
    pub type_name: String,
    pub from_generation: u32,
    pub to_generation: u32,
    pub target_type_name: Option<String>,
    pub field_migrations: Vec<FieldMigration>,
}

impl SignalMigration {
    pub fn new(type_name: impl Into<String>, from_gen: u32, to_gen: u32) -> Self {
        Self {
            type_name: type_name.into(),
            from_generation: from_gen,
            to_generation: to_gen,
            target_type_name: None,
            field_migrations: Vec::new(),
        }
    }

    pub fn with_target_type(mut self, target: impl Into<String>) -> Self {
        self.target_type_name = Some(target.into());
        self
    }

    pub fn rename(mut self, old_name: impl Into<String>, new_name: impl Into<String>) -> Self {
        self.field_migrations.push(FieldMigration {
            name: old_name.into(),
            action: FieldAction::Rename {
                new_name: new_name.into(),
            },
        });
        self
    }

    pub fn add_default(
        mut self,
        name: impl Into<String>,
        default_value: GenerationalFieldValue,
    ) -> Self {
        self.field_migrations.push(FieldMigration {
            name: name.into(),
            action: FieldAction::AddDefault { default_value },
        });
        self
    }

    pub fn remove(mut self, name: impl Into<String>) -> Self {
        self.field_migrations.push(FieldMigration {
            name: name.into(),
            action: FieldAction::Remove,
        });
        self
    }

    pub fn convert<F>(mut self, name: impl Into<String>, f: F) -> Self
    where
        F: Fn(GenerationalFieldValue) -> GenerationalFieldValue + Send + Sync + 'static,
    {
        self.field_migrations.push(FieldMigration {
            name: name.into(),
            action: FieldAction::Convert(Arc::new(f)),
        });
        self
    }

    /// Applies migration to a signal snapshot if the type and generation match.
    pub fn apply(&self, signal: &mut SignalSnapshot) -> bool {
        if signal.type_name != self.type_name || signal.generation != self.from_generation {
            return false;
        }

        for fm in &self.field_migrations {
            match &fm.action {
                FieldAction::Rename { new_name } => {
                    if let Some(val) = signal.fields.remove(&fm.name) {
                        signal.fields.insert(new_name.clone(), val);
                    }
                }
                FieldAction::AddDefault { default_value } => {
                    signal
                        .fields
                        .entry(fm.name.clone())
                        .or_insert_with(|| default_value.clone());
                }
                FieldAction::Remove => {
                    signal.fields.remove(&fm.name);
                }
                FieldAction::Convert(converter) => {
                    if let Some(val) = signal.fields.remove(&fm.name) {
                        let converted = converter(val);
                        signal.fields.insert(fm.name.clone(), converted);
                    }
                }
            }
        }

        if let Some(ref target_type) = self.target_type_name {
            signal.type_name = target_type.clone();
        }
        signal.generation = self.to_generation;
        true
    }
}

/// Serialized state of an individual reactive signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalSnapshot {
    pub signal_id: u64,
    pub type_name: String,
    pub generation: u32,
    pub value_bytes: Vec<u8>,
    pub fields: HashMap<String, GenerationalFieldValue>,
}

impl SignalSnapshot {
    pub fn get_field(&self, name: &str) -> Option<&GenerationalFieldValue> {
        self.fields.get(name)
    }

    pub fn set_field(&mut self, name: impl Into<String>, value: GenerationalFieldValue) {
        self.fields.insert(name.into(), value);
    }
}

/// Serialized state of a component instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentSnapshot {
    pub key: String,
    pub memo_index: usize,
    pub state_data: HashMap<String, String>,
}

/// Comprehensive state snapshot of the active application.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HotReloadSnapshot {
    pub epoch: u64,
    pub signals: HashMap<u64, SignalSnapshot>,
    pub components: HashMap<String, ComponentSnapshot>,
}

impl HotReloadSnapshot {
    pub fn new(epoch: u64) -> Self {
        Self {
            epoch,
            signals: HashMap::new(),
            components: HashMap::new(),
        }
    }

    /// Inserts raw signal bytes (defaulting to generation 1).
    pub fn insert_signal(
        &mut self,
        signal_id: u64,
        type_name: impl Into<String>,
        value_bytes: Vec<u8>,
    ) {
        self.signals.insert(
            signal_id,
            SignalSnapshot {
                signal_id,
                type_name: type_name.into(),
                generation: 1,
                value_bytes,
                fields: HashMap::new(),
            },
        );
    }

    /// Inserts a structured generational signal with field definitions.
    pub fn insert_generational_signal(
        &mut self,
        signal_id: u64,
        type_name: impl Into<String>,
        generation: u32,
        fields: HashMap<String, GenerationalFieldValue>,
    ) {
        self.signals.insert(
            signal_id,
            SignalSnapshot {
                signal_id,
                type_name: type_name.into(),
                generation,
                value_bytes: Vec::new(),
                fields,
            },
        );
    }

    /// Inserts component state snapshot.
    pub fn insert_component(
        &mut self,
        key: impl Into<String>,
        memo_index: usize,
        state: HashMap<String, String>,
    ) {
        let k = key.into();
        self.components.insert(
            k.clone(),
            ComponentSnapshot {
                key: k,
                memo_index,
                state_data: state,
            },
        );
    }

    /// Applies schema migrations across all matching signals.
    /// Iterates until all transitive migrations have settled.
    pub fn migrate_signals(&mut self, migrations: &[SignalMigration]) -> usize {
        let mut total_migrated = 0;
        let mut progressed = true;
        while progressed {
            progressed = false;
            for sig in self.signals.values_mut() {
                for mig in migrations {
                    if mig.apply(sig) {
                        total_migrated += 1;
                        progressed = true;
                        break;
                    }
                }
            }
        }
        total_migrated
    }

    /// Serializes snapshot into a compact byte payload with generational support.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Epoch (8 bytes)
        buf.extend_from_slice(&self.epoch.to_le_bytes());

        // Generational V2 format flag: high bit set on signals count
        let signals_count = (self.signals.len() as u32) | 0x8000_0000;
        buf.extend_from_slice(&signals_count.to_le_bytes());

        for s in self.signals.values() {
            buf.extend_from_slice(&s.signal_id.to_le_bytes());
            let type_bytes = s.type_name.as_bytes();
            buf.extend_from_slice(&(type_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(type_bytes);

            // Generation counter
            buf.extend_from_slice(&s.generation.to_le_bytes());

            // Value bytes
            buf.extend_from_slice(&(s.value_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(&s.value_bytes);

            // Generational fields
            buf.extend_from_slice(&(s.fields.len() as u32).to_le_bytes());
            for (k, v) in &s.fields {
                let kb = k.as_bytes();
                buf.extend_from_slice(&(kb.len() as u16).to_le_bytes());
                buf.extend_from_slice(kb);
                v.encode(&mut buf);
            }
        }

        // Components count
        buf.extend_from_slice(&(self.components.len() as u32).to_le_bytes());
        for c in self.components.values() {
            let key_bytes = c.key.as_bytes();
            buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(key_bytes);
            buf.extend_from_slice(&(c.memo_index as u64).to_le_bytes());
            buf.extend_from_slice(&(c.state_data.len() as u16).to_le_bytes());
            for (k, v) in &c.state_data {
                let kb = k.as_bytes();
                buf.extend_from_slice(&(kb.len() as u16).to_le_bytes());
                buf.extend_from_slice(kb);
                let vb = v.as_bytes();
                buf.extend_from_slice(&(vb.len() as u16).to_le_bytes());
                buf.extend_from_slice(vb);
            }
        }

        buf
    }

    /// Deserializes a byte slice into a HotReloadSnapshot, seamlessly supporting both v1 and v2.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 + 4 {
            return None;
        }
        let mut offset = 0;
        let epoch = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
        offset += 8;

        let raw_signals = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
        offset += 4;

        let is_v2 = (raw_signals & 0x8000_0000) != 0;
        let num_signals = (raw_signals & 0x7FFF_FFFF) as usize;

        let mut signals = HashMap::with_capacity(num_signals);
        for _ in 0..num_signals {
            if bytes.len() < offset + 8 + 2 {
                return None;
            }
            let signal_id = u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?);
            offset += 8;
            let type_len = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
            offset += 2;
            if bytes.len() < offset + type_len {
                return None;
            }
            let type_name = String::from_utf8(bytes[offset..offset + type_len].to_vec()).ok()?;
            offset += type_len;

            let generation = if is_v2 {
                if bytes.len() < offset + 4 {
                    return None;
                }
                let gen = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
                offset += 4;
                gen
            } else {
                1
            };

            if bytes.len() < offset + 4 {
                return None;
            }
            let val_len = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
            offset += 4;
            if bytes.len() < offset + val_len {
                return None;
            }
            let value_bytes = bytes[offset..offset + val_len].to_vec();
            offset += val_len;

            let mut fields = HashMap::new();
            if is_v2 {
                if bytes.len() < offset + 4 {
                    return None;
                }
                let num_fields =
                    u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
                offset += 4;
                for _ in 0..num_fields {
                    if bytes.len() < offset + 2 {
                        return None;
                    }
                    let klen =
                        u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
                    offset += 2;
                    if bytes.len() < offset + klen {
                        return None;
                    }
                    let k = String::from_utf8(bytes[offset..offset + klen].to_vec()).ok()?;
                    offset += klen;
                    let v = GenerationalFieldValue::decode(bytes, &mut offset)?;
                    fields.insert(k, v);
                }
            }

            signals.insert(
                signal_id,
                SignalSnapshot {
                    signal_id,
                    type_name,
                    generation,
                    value_bytes,
                    fields,
                },
            );
        }

        if bytes.len() < offset + 4 {
            return None;
        }
        let num_components =
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        offset += 4;

        let mut components = HashMap::with_capacity(num_components);
        for _ in 0..num_components {
            if bytes.len() < offset + 2 {
                return None;
            }
            let key_len = u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
            offset += 2;
            if bytes.len() < offset + key_len + 8 + 2 {
                return None;
            }
            let key = String::from_utf8(bytes[offset..offset + key_len].to_vec()).ok()?;
            offset += key_len;
            let memo_index =
                u64::from_le_bytes(bytes[offset..offset + 8].try_into().ok()?) as usize;
            offset += 8;
            let num_entries =
                u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
            offset += 2;

            let mut state_data = HashMap::with_capacity(num_entries);
            for _ in 0..num_entries {
                if bytes.len() < offset + 2 {
                    return None;
                }
                let k_len =
                    u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
                offset += 2;
                if bytes.len() < offset + k_len + 2 {
                    return None;
                }
                let k = String::from_utf8(bytes[offset..offset + k_len].to_vec()).ok()?;
                offset += k_len;
                let v_len =
                    u16::from_le_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
                offset += 2;
                if bytes.len() < offset + v_len {
                    return None;
                }
                let v = String::from_utf8(bytes[offset..offset + v_len].to_vec()).ok()?;
                offset += v_len;
                state_data.insert(k, v);
            }

            components.insert(
                key.clone(),
                ComponentSnapshot {
                    key,
                    memo_index,
                    state_data,
                },
            );
        }

        Some(HotReloadSnapshot {
            epoch,
            signals,
            components,
        })
    }
}
