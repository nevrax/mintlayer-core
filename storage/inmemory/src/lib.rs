// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use storage_core::{adaptor, backend, Data, DbDesc, DbIndex};

use std::collections::BTreeMap;

type Map = BTreeMap<Data, Data>;

pub struct StorageMaps(Vec<Map>);

impl backend::ReadOps for StorageMaps {
    fn get(&self, idx: DbIndex, key: &[u8]) -> storage_core::Result<Option<&[u8]>> {
        Ok(self.0[idx.get()].get(key).map(AsRef::as_ref))
    }
}

pub struct PrefixIter<'m> {
    inner: std::collections::btree_map::Range<'m, Data, Data>,
    prefix: Data,
}

impl<'m> PrefixIter<'m> {
    fn new(map: &'m Map, prefix: Data) -> Self {
        let inner = map.range(prefix.clone()..);
        Self { inner, prefix }
    }
}

impl Iterator for PrefixIter<'_> {
    type Item = (Data, Data);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .and_then(|(k, v)| k.starts_with(&self.prefix).then(|| (k.clone(), v.clone())))
    }
}

impl<'i> backend::PrefixIter<'i> for StorageMaps {
    // TODO: Define a custom iterator type and do this without boxing
    type Iterator = PrefixIter<'i>;

    fn prefix_iter<'m: 'i>(
        &'m self,
        idx: DbIndex,
        prefix: Data,
    ) -> storage_core::Result<Self::Iterator> {
        Ok(PrefixIter::new(&self.0[idx.get()], prefix))
    }
}

impl backend::WriteOps for StorageMaps {
    fn put(&mut self, idx: DbIndex, key: Data, val: Data) -> storage_core::Result<()> {
        let _ = self.0[idx.get()].insert(key, val);
        Ok(())
    }

    fn del(&mut self, idx: DbIndex, key: &[u8]) -> storage_core::Result<()> {
        let _ = self.0[idx.get()].remove(key);
        Ok(())
    }
}

impl adaptor::Construct for StorageMaps {
    type From = ();

    fn construct(_: (), desc: DbDesc) -> storage_core::Result<Self> {
        Ok(Self(vec![Map::new(); desc.len()]))
    }
}

#[derive(Clone)]
pub struct InMemory(adaptor::Locking<StorageMaps>);

impl backend::Backend for InMemory {
    type Impl = <adaptor::Locking<StorageMaps> as backend::Backend>::Impl;

    fn open(self, desc: DbDesc) -> storage_core::Result<Self::Impl> {
        self.0.open(desc)
    }
}

impl InMemory {
    /// Create a new in-memory storage backend
    pub fn new() -> Self {
        Self(adaptor::Locking::new(()))
    }
}

impl Default for InMemory {
    fn default() -> Self {
        Self::new()
    }
}
