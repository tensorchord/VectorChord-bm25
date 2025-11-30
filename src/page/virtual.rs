use super::{
    page_alloc_init_forknum, page_alloc_with_fsm, page_read, page_write, PageFlags, PageWriteGuard,
    BM25_PAGE_SIZE,
};

const DIRECT_COUNT: usize = BM25_PAGE_SIZE / 4;
const INDIRECT1_COUNT: usize = DIRECT_COUNT * DIRECT_COUNT;
const INDIRECT2_COUNT: usize = INDIRECT1_COUNT * DIRECT_COUNT;

#[inline]
fn read_u32_entry(data: &[u8], idx: usize) -> u32 {
    let slice = &data[idx * 4..][..4];
    u32::from_le_bytes(slice.try_into().unwrap())
}

#[inline]
fn read_last_u32(data: &[u8]) -> u32 {
    let slice = &data[data.len() - 4..];
    u32::from_le_bytes(slice.try_into().unwrap())
}

#[inline]
fn try_append_inode_entry(page: &mut PageWriteGuard, blkno: u32) -> bool {
    let space = page.freespace_mut();
    if space.len() < 4 {
        return false;
    }
    space[..4].copy_from_slice(&blkno.to_le_bytes());
    page.header.pd_lower += 4;
    true
}

#[derive(Debug)]
pub struct VirtualPageReader {
    relation: pgrx::pg_sys::Relation,
    direct_inode: Box<[u32]>,
    indirect1_inode_blkno: u32,
}

impl VirtualPageReader {
    pub fn new(relation: pgrx::pg_sys::Relation, blkno: u32) -> Self {
        assert!(blkno != pgrx::pg_sys::InvalidBlockNumber);
        let direct_inode_page = page_read(relation, blkno);
        let data = direct_inode_page.data();
        let mut direct_inode: Vec<u32> = Vec::with_capacity(data.len() / 4);
        direct_inode.extend_from_slice(bytemuck::cast_slice(data));
        let direct_inode = direct_inode.into_boxed_slice();
        let indirect1_inode_blkno = direct_inode_page.opaque.next_blkno;

        Self {
            relation,
            direct_inode,
            indirect1_inode_blkno,
        }
    }

    pub fn read_at(&self, offset: u32, buf: &mut [u8]) {
        let virtual_id = offset / BM25_PAGE_SIZE as u32;
        let page_offset = offset % BM25_PAGE_SIZE as u32;
        assert!(page_offset + buf.len() as u32 <= BM25_PAGE_SIZE as u32);
        let block_id = self.get_block_id(virtual_id);
        let block = page_read(self.relation, block_id);
        let data = &block.data()[page_offset as usize..][..buf.len()];
        buf.copy_from_slice(data);
    }

    pub fn update_at(&self, offset: u32, len: u32, f: impl FnOnce(&mut [u8])) {
        let virtual_id = offset / BM25_PAGE_SIZE as u32;
        let page_offset = offset % BM25_PAGE_SIZE as u32;
        assert!(page_offset + len <= BM25_PAGE_SIZE as u32);
        let block_id = self.get_block_id(virtual_id);
        let mut block = page_write(self.relation, block_id);
        let data = &mut block.data_mut()[page_offset as usize..][..len as usize];
        f(data);
    }

    pub fn get_block_id(&self, virtual_id: u32) -> u32 {
        let mut virtual_id = virtual_id as usize;
        if virtual_id < DIRECT_COUNT {
            return self.direct_inode[virtual_id];
        }

        virtual_id -= DIRECT_COUNT;
        let indirect1_inode = page_read(self.relation, self.indirect1_inode_blkno);
        if virtual_id < INDIRECT1_COUNT {
            let indirect1_id = virtual_id / DIRECT_COUNT;
            let indirect1_offset = virtual_id % DIRECT_COUNT;
            let blkno = read_u32_entry(indirect1_inode.data(), indirect1_id);
            let indirect = page_read(self.relation, blkno);
            return read_u32_entry(indirect.data(), indirect1_offset);
        }

        virtual_id -= INDIRECT1_COUNT;
        assert!(virtual_id < INDIRECT2_COUNT);
        let indirect2_inode = page_read(self.relation, indirect1_inode.opaque.next_blkno);
        let indirect2_id = virtual_id / INDIRECT1_COUNT;
        let indirect2_offset = virtual_id % INDIRECT1_COUNT;
        let indirect1_id = indirect2_offset / DIRECT_COUNT;
        let indirect1_offset = indirect2_offset % DIRECT_COUNT;

        let blkno = read_u32_entry(indirect2_inode.data(), indirect2_id);
        let indirect1 = page_read(self.relation, blkno);
        let blkno = read_u32_entry(indirect1.data(), indirect1_id);
        let indirect = page_read(self.relation, blkno);
        read_u32_entry(indirect.data(), indirect1_offset)
    }
}

enum VirtualPageWriterState {
    Direct([PageWriteGuard; 2]),
    Indirect1([PageWriteGuard; 3]),
    Indirect2([PageWriteGuard; 4]),
}

pub struct VirtualPageWriter {
    relation: pgrx::pg_sys::Relation,
    flag: PageFlags,
    skip_lock_rel: bool,
    first_blkno: pgrx::pg_sys::BlockNumber,
    state: VirtualPageWriterState,
}

impl VirtualPageWriter {
    pub fn init_fork(relation: pgrx::pg_sys::Relation, flag: PageFlags) -> u32 {
        let mut direct_inode = page_alloc_init_forknum(relation, flag);
        let data_page = page_alloc_init_forknum(relation, flag);
        let first_blkno = direct_inode.blkno();
        direct_inode.freespace_mut()[..4].copy_from_slice(&data_page.blkno().to_le_bytes());
        direct_inode.header.pd_lower += 4;
        first_blkno
    }

    pub fn new(relation: pgrx::pg_sys::Relation, flag: PageFlags, skip_lock_rel: bool) -> Self {
        let mut direct_inode = page_alloc_with_fsm(relation, flag, skip_lock_rel);
        let data_page = page_alloc_with_fsm(relation, flag, skip_lock_rel);
        let first_blkno = direct_inode.blkno();
        direct_inode.freespace_mut()[..4].copy_from_slice(&data_page.blkno().to_le_bytes());
        direct_inode.header.pd_lower += 4;

        Self {
            relation,
            flag,
            skip_lock_rel,
            first_blkno,
            state: VirtualPageWriterState::Direct([data_page, direct_inode]),
        }
    }

    pub fn open(relation: pgrx::pg_sys::Relation, first_blkno: u32, skip_lock_rel: bool) -> Self {
        let direct_inode = page_read(relation, first_blkno);
        let flag = direct_inode.opaque.page_flag;
        let indirect1_blkno = direct_inode.opaque.next_blkno;
        drop(direct_inode);
        if indirect1_blkno == pgrx::pg_sys::InvalidBlockNumber {
            let direct_inode = page_write(relation, first_blkno);
            let data_page_id = read_last_u32(direct_inode.data());
            let data_page = page_write(relation, data_page_id);
            return Self {
                relation,
                flag,
                skip_lock_rel,
                first_blkno,
                state: VirtualPageWriterState::Direct([data_page, direct_inode]),
            };
        }

        let indirect1_inode = page_read(relation, indirect1_blkno);
        let indirect2_blkno = indirect1_inode.opaque.next_blkno;
        drop(indirect1_inode);
        if indirect2_blkno == pgrx::pg_sys::InvalidBlockNumber {
            let indirect1_inode = page_write(relation, indirect1_blkno);
            let indirect1_page_id = read_last_u32(indirect1_inode.data());
            let indirect1_page = page_write(relation, indirect1_page_id);
            let data_page_id = read_last_u32(indirect1_page.data());
            let data_page = page_write(relation, data_page_id);
            return Self {
                relation,
                flag,
                skip_lock_rel,
                first_blkno,
                state: VirtualPageWriterState::Indirect1([
                    data_page,
                    indirect1_page,
                    indirect1_inode,
                ]),
            };
        }

        let indirect2_inode = page_write(relation, indirect2_blkno);
        let indirect2_page_id = read_last_u32(indirect2_inode.data());
        let indirect2_page = page_write(relation, indirect2_page_id);
        let indirect1_page_id = read_last_u32(indirect2_page.data());
        let indirect1_page = page_write(relation, indirect1_page_id);
        let data_page_id = read_last_u32(indirect1_page.data());
        let data_page = page_write(relation, data_page_id);
        Self {
            relation,
            flag,
            skip_lock_rel,
            first_blkno,
            state: VirtualPageWriterState::Indirect2([
                data_page,
                indirect1_page,
                indirect2_page,
                indirect2_inode,
            ]),
        }
    }

    pub fn page_count(&self) -> usize {
        match &self.state {
            VirtualPageWriterState::Direct([_, direct]) => direct.data().len() / 4,
            VirtualPageWriterState::Indirect1([_, indirect1_page, indirect1_inode]) => {
                indirect1_page.data().len() / 4
                    + (indirect1_inode.data().len() / 4 - 1) * DIRECT_COUNT
            }
            VirtualPageWriterState::Indirect2(
                [_, indirect1_page, indirect2_page, indirect2_inode],
            ) => {
                indirect1_page.data().len() / 4
                    + (indirect2_page.data().len() / 4 - 1) * DIRECT_COUNT
                    + (indirect2_inode.data().len() / 4 - 1) * INDIRECT1_COUNT
            }
        }
    }

    pub fn finalize(self) -> u32 {
        self.first_blkno
    }

    pub fn first_blkno(&self) -> u32 {
        self.first_blkno
    }

    pub fn write(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let mut space = self.freespace_mut();
            if space.is_empty() {
                self.new_page();
                space = self.freespace_mut();
            }
            let space_len = space.len();
            let len = space_len.min(data.len());
            space[..len].copy_from_slice(&data[..len]);
            *self.offset() += len as u16;
            data = &data[len..];
        }
    }

    pub fn write_vectorized_no_cross(&mut self, data: &[&[u8]]) -> bool {
        let mut change_page = false;
        let len = data.iter().map(|d| d.len()).sum::<usize>();
        assert!(len <= BM25_PAGE_SIZE);
        let mut space = self.freespace_mut();
        if space.len() < len {
            change_page = true;
            self.new_page();
            space = self.freespace_mut();
        }
        let mut offset = 0;
        for d in data {
            space[offset..][..d.len()].copy_from_slice(d);
            offset += d.len();
        }
        *self.offset() += len as u16;
        change_page
    }

    fn offset(&mut self) -> &mut u16 {
        &mut self.data_page().header.pd_lower
    }

    fn freespace_mut(&mut self) -> &mut [u8] {
        match &mut self.state {
            VirtualPageWriterState::Direct([page, _]) => page.freespace_mut(),
            VirtualPageWriterState::Indirect1([page, _, _]) => page.freespace_mut(),
            VirtualPageWriterState::Indirect2([page, _, _, _]) => page.freespace_mut(),
        }
    }

    fn new_page(&mut self) {
        match &mut self.state {
            VirtualPageWriterState::Direct([old_data_page, direct_inode]) => {
                let data_page = page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                old_data_page.opaque.next_blkno = data_page.blkno();
                if try_append_inode_entry(direct_inode, data_page.blkno()) {
                    *old_data_page = data_page;
                    return;
                }

                let mut indirect1_inode =
                    page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                direct_inode.opaque.next_blkno = indirect1_inode.blkno();

                let mut indirect1_page =
                    page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                // First entry points to the first indirect1 data page
                try_append_inode_entry(&mut indirect1_inode, indirect1_page.blkno());
                // First data entry points to the new data page
                try_append_inode_entry(&mut indirect1_page, data_page.blkno());

                self.state =
                    VirtualPageWriterState::Indirect1([data_page, indirect1_page, indirect1_inode]);
            }
            VirtualPageWriterState::Indirect1([old_data_page, indirect1_page, indirect1_inode]) => {
                let data_page = page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                old_data_page.opaque.next_blkno = data_page.blkno();
                if try_append_inode_entry(indirect1_page, data_page.blkno()) {
                    *old_data_page = data_page;
                    return;
                }

                let mut new_indirect1_page =
                    page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                try_append_inode_entry(&mut new_indirect1_page, data_page.blkno());

                if try_append_inode_entry(indirect1_inode, new_indirect1_page.blkno()) {
                    *old_data_page = data_page;
                    *indirect1_page = new_indirect1_page;
                    return;
                }

                let mut indirect2_inode =
                    page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                indirect1_inode.opaque.next_blkno = indirect2_inode.blkno();

                let mut indirect2_page =
                    page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                try_append_inode_entry(&mut indirect2_inode, indirect2_page.blkno());
                try_append_inode_entry(&mut indirect2_page, new_indirect1_page.blkno());

                self.state = VirtualPageWriterState::Indirect2([
                    data_page,
                    new_indirect1_page,
                    indirect2_page,
                    indirect2_inode,
                ]);
            }
            VirtualPageWriterState::Indirect2(
                [old_data_page, indirect1_page, indirect2_page, indirect2_inode],
            ) => {
                let data_page = page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                old_data_page.opaque.next_blkno = data_page.blkno();
                if try_append_inode_entry(indirect1_page, data_page.blkno()) {
                    *old_data_page = data_page;
                    return;
                }

                let mut new_indirect1_page =
                    page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                let _ = try_append_inode_entry(&mut new_indirect1_page, data_page.blkno());

                if try_append_inode_entry(indirect2_page, new_indirect1_page.blkno()) {
                    *old_data_page = data_page;
                    *indirect1_page = new_indirect1_page;
                    return;
                }

                let mut new_indirect2_page =
                    page_alloc_with_fsm(self.relation, self.flag, self.skip_lock_rel);
                try_append_inode_entry(&mut new_indirect2_page, new_indirect1_page.blkno());

                if try_append_inode_entry(indirect2_inode, new_indirect2_page.blkno()) {
                    *old_data_page = data_page;
                    *indirect1_page = new_indirect1_page;
                    *indirect2_page = new_indirect2_page;
                    return;
                }

                panic!("VirtualPageWriter: too many pages");
            }
        }
    }

    pub fn data_page(&mut self) -> &mut PageWriteGuard {
        match &mut self.state {
            VirtualPageWriterState::Direct(pages) => &mut pages[0],
            VirtualPageWriterState::Indirect1(pages) => &mut pages[0],
            VirtualPageWriterState::Indirect2(pages) => &mut pages[0],
        }
    }
}
