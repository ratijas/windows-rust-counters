use std::cmp::Ordering;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::perf::nom::PerfInstanceDefinition;
use crate::perf::provide::{PerfCounterDefinitionTemplate, PerfInstanceDefinitionTemplate};
use crate::perf::values::*;
use crate::prelude::v2::*;

/// Wrapper around `NumInstances` attribute.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum NumInstances {
    /// Corresponds to `PERF_NO_INSTANCES`.
    NoInstances,
    /// Signed non-negative number of instances.
    /// Zero means that no instances available at the moment.
    N(u32),
}

impl NumInstances {
    pub fn has_instances(&self) -> bool {
        matches!(self, NumInstances::NoInstances)
    }

    /// Total number of `PERF_COUNTER_BLOCK`s contained in the corresponding `PERF_OBJECT_TYPE`.
    pub fn num_counter_blocks(&self) -> u32 {
        match self {
            Self::NoInstances => 1,
            Self::N(n) => *n as _,
        }
    }
}

impl TryFrom<i32> for NumInstances {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            PERF_NO_INSTANCES => Ok(Self::NoInstances),
            n if n >= 0 && n <= 1024 => Ok(Self::N(n as _)),
            _ => Err(()),
        }
    }
}

impl From<NumInstances> for i32 {
    fn from(this: NumInstances) -> Self {
        match this {
            NumInstances::NoInstances => PERF_NO_INSTANCES,
            NumInstances::N(n) => n as _,
        }
    }
}

/// Just enough information to identify an instance among all instances of an object.
#[derive(Clone, Debug, Eq)]
pub struct InstanceId {
    /// UniqueID field of `PERF_INSTANCE_DEFINITION` block.
    /// When id is PERF_NO_UNIQUE_ID, then
    /// name will be used for search and comparison.
    unique_id: i32,
    /// Name of instance. When `unique_id` is not available,
    /// name will be used for search and comparison.
    /// Only one instance without name may exist in one object.
    name: U16CString,
}

impl InstanceId {
    /// Construct `InstanceId` from raw parts. Prefer using `From` conversions instead.
    pub fn new(unique_id: i32, name: &U16CStr) -> Self {
        InstanceId {
            unique_id,
            name: name.to_owned(),
        }
    }

    /// `InstanceId` for an implicit global instance when number of instances is `PERF_NO_INSTANCES`.
    pub fn perf_no_instances() -> Self {
        InstanceId::new(PERF_NO_UNIQUE_ID, u16cstr!(""))
    }

    /// `None` iff unique ID is `PERF_NO_UNIQUE_ID`.
    pub fn unique_id(&self) -> Option<i32> {
        match self.unique_id {
            PERF_NO_UNIQUE_ID => None,
            id => Some(id),
        }
    }

    /// Get borrowed name of this instance.
    pub fn name(&self) -> &U16CStr {
        &self.name
    }
}

impl<'a> From<&PerfInstanceDefinitionTemplate<'a>> for InstanceId {
    fn from(value: &PerfInstanceDefinitionTemplate<'a>) -> Self {
        InstanceId::new(value.UniqueID, &value.Name)
    }
}

impl<'a> From<&PerfInstanceDefinition> for InstanceId {
    fn from(def: &PerfInstanceDefinition) -> Self {
        InstanceId::new(def.UniqueID, &def.name)
    }
}

impl<'a> From<&'a InstanceId> for PerfInstanceDefinitionTemplate<'a> {
    fn from(instance: &'a InstanceId) -> Self {
        let mut this = PerfInstanceDefinitionTemplate::new(instance.name().into());
        if let Some(unique_id) = instance.unique_id() {
            this = this.with_unique_id(unique_id);
        }
        this
    }
}

impl PartialEq for InstanceId {
    fn eq(&self, other: &Self) -> bool {
        let self_valid = self.unique_id().is_some();
        let other_valid = other.unique_id().is_some();

        if self_valid && other_valid {
            // both are comparable by UniqueID
            self.unique_id == other.unique_id
        } else if self_valid || other_valid {
            // one is valid, other is not
            false
        } else {
            // both have invalid UniqueID
            self.name == other.name
        }
    }
}

impl PartialOrd for InstanceId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let self_valid = self.unique_id().is_some();
        let other_valid = other.unique_id().is_some();

        if self_valid && other_valid {
            // both are comparable by UniqueID
            Some(self.unique_id.cmp(&other.unique_id))
        } else if self_valid || other_valid {
            // one is valid, other is not
            None
        } else {
            // both have invalid UniqueID
            Some(self.name.cmp(&other.name))
        }
    }
}

impl Ord for InstanceId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or_else(|| {
            self.unique_id()
                .map(|_| Ordering::Greater)
                .unwrap_or(Ordering::Less)
        })
    }
}

impl Hash for InstanceId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.unique_id() {
            Some(id) => (id.wrapping_mul(37)).hash(state),
            None => self.name.hash(state),
        }
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self == &Self::perf_no_instances() {
            "PERF_NO_INSTANCES".fmt(f)
        } else {
            self.name.to_string_lossy().fmt(f)
        }
    }
}

/// Just enough information to identify a counter among all counters of an object.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CounterId {
    /// `CounterNameTitleIndex` before adding index of first counter.
    /// Essentially, it is the index defined in the header / symbols file.
    name_index_offset: u32,
}

impl CounterId {
    pub fn new(name_index_offset: u32) -> Self {
        CounterId { name_index_offset }
    }
}

impl From<&PerfCounterDefinitionTemplate> for CounterId {
    fn from(value: &PerfCounterDefinitionTemplate) -> Self {
        CounterId::new(value.name_offset)
    }
}

impl From<u32> for CounterId {
    fn from(value: u32) -> Self {
        CounterId::new(value)
    }
}

#[derive(Debug, Clone)]
pub struct ObjectData {
    inner: HashMap<(CounterId, InstanceId), CounterValue>,
}

impl ObjectData {
    pub fn new() -> Self {
        ObjectData {
            inner: HashMap::new(),
        }
    }

    pub fn get<'a>(
        &'a self,
        counter_id: CounterId,
        instance_id: InstanceId,
    ) -> Option<CounterVal<'a>> {
        self.inner
            .get(&(counter_id, instance_id))
            .map(|owned| owned.borrow())
    }

    pub fn set(&mut self, counter_id: CounterId, instance_id: InstanceId, value: CounterValue) {
        self.inner.insert((counter_id, instance_id), value);
    }
}

#[derive(Clone)]
pub struct SharedObjectData {
    inner: Arc<Mutex<ObjectData>>,
}

impl SharedObjectData {
    pub fn new() -> Self {
        SharedObjectData {
            inner: Arc::new(Mutex::new(ObjectData::new())),
        }
    }

    pub fn read(&self) -> ObjectData {
        let lock = self.inner.lock().unwrap();
        lock.clone()
    }

    pub fn update(&self, f: impl FnOnce(ObjectData) -> ObjectData) {
        let mut lock = self.inner.lock().unwrap();
        let old = lock.clone();
        let new = f(old);
        *lock = new;
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_hash_counter_id() {
        let c1 = CounterId::new(42);
        let c2 = CounterId::new(42);
        let c3 = CounterId::new(37);

        assert_eq!(c1, c2);
        assert_ne!(c1, c3);
        assert_ne!(c2, c3);
    }

    #[test]
    fn test_hash_instance_id() {
        let i1 = InstanceId::new(42, u16cstr!("abc"));
        let i2 = InstanceId::new(42, u16cstr!("abc"));
        let i3 = InstanceId::new(37, u16cstr!("abc"));
        let i4_a = InstanceId::new(PERF_NO_UNIQUE_ID, u16cstr!("a"));
        let i4_b = InstanceId::new(PERF_NO_UNIQUE_ID, u16cstr!("b"));
        let i5 = InstanceId::perf_no_instances();
        let i6 = InstanceId::perf_no_instances();

        assert_eq!(i1, i2, "should be equal");
        assert_ne!(i2, i3, "use ID when available");
        assert_ne!(i4_a, i4_b, "use name when no ID");
        assert_eq!(i5, i6);

        let mut map: HashMap<InstanceId, u32> = HashMap::new();
        map.insert(i1, 123);
        assert_eq!(map[&i2], 123);
        map.insert(i4_a, 456);
        assert_eq!(map.get(&i4_b), None);
    }
}
