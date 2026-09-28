//! 共享任务行存储：controller 增量写、表格 / 详情按下标读，不逐事件全量克隆。

use std::{
    cell::{Cell, Ref, RefCell},
    collections::HashMap,
};

use super::{DownloadTaskView, RowKey};

/// 行在存储中的位置（随删除会变；跨帧身份用 [`RowKey`]）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum RowId {
    Local(usize),
    Remote(usize),
}

#[derive(Default)]
pub(crate) struct TaskStore {
    local: RefCell<Vec<DownloadTaskView>>,
    remote: RefCell<Vec<DownloadTaskView>>,
    /// 本地 task_id → `local` 下标。
    index: RefCell<HashMap<String, usize>>,
    generation: Cell<u64>,
}

impl TaskStore {
    pub(crate) fn local(&self) -> Ref<'_, [DownloadTaskView]> {
        Ref::map(self.local.borrow(), Vec::as_slice)
    }

    pub(crate) fn remote(&self) -> Ref<'_, [DownloadTaskView]> {
        Ref::map(self.remote.borrow(), Vec::as_slice)
    }

    pub(crate) fn row(&self, id: RowId) -> Option<Ref<'_, DownloadTaskView>> {
        match id {
            RowId::Local(ix) => Ref::filter_map(self.local.borrow(), |rows| rows.get(ix)).ok(),
            RowId::Remote(ix) => Ref::filter_map(self.remote.borrow(), |rows| rows.get(ix)).ok(),
        }
    }

    pub(crate) fn row_id(&self, key: &RowKey) -> Option<RowId> {
        match key {
            RowKey::Local(id) => self.find_local(id).map(RowId::Local),
            RowKey::Remote(id) => self
                .remote
                .borrow()
                .iter()
                .position(|row| row.key.task_id() == id)
                .map(RowId::Remote),
        }
    }

    pub(crate) fn get(&self, key: &RowKey) -> Option<Ref<'_, DownloadTaskView>> {
        self.row_id(key).and_then(|id| self.row(id))
    }

    pub(crate) fn find_local(&self, task_id: &str) -> Option<usize> {
        self.index.borrow().get(task_id).copied()
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation.get()
    }

    fn bump(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
    }

    pub(crate) fn replace_local(&self, rows: Vec<DownloadTaskView>) {
        let index = rows
            .iter()
            .enumerate()
            .map(|(ix, row)| (row.key.task_id().to_owned(), ix))
            .collect();
        *self.index.borrow_mut() = index;
        *self.local.borrow_mut() = rows;
        self.bump();
    }

    pub(crate) fn replace_remote(&self, rows: Vec<DownloadTaskView>) {
        *self.remote.borrow_mut() = rows;
        self.bump();
    }

    /// 覆盖单行（下标必须已存在）。
    pub(crate) fn set_local(&self, ix: usize, row: DownloadTaskView) {
        if let Some(slot) = self.local.borrow_mut().get_mut(ix) {
            *slot = row;
            self.bump();
        }
    }

    pub(crate) fn push_local(&self, row: DownloadTaskView) -> usize {
        let mut rows = self.local.borrow_mut();
        let ix = rows.len();
        self.index
            .borrow_mut()
            .insert(row.key.task_id().to_owned(), ix);
        rows.push(row);
        self.bump();
        ix
    }

    /// `swap_remove` 并修正被换入行的索引。
    pub(crate) fn swap_remove_local(&self, ix: usize) {
        let mut rows = self.local.borrow_mut();
        if ix >= rows.len() {
            return;
        }
        let removed = rows.swap_remove(ix);
        let mut index = self.index.borrow_mut();
        index.remove(removed.key.task_id());
        if let Some(moved) = rows.get(ix) {
            index.insert(moved.key.task_id().to_owned(), ix);
        }
        self.bump();
    }
}
