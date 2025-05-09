// Copyright 2014 Tyler Neely
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

use compaction_filter::{
    new_compaction_filter, new_compaction_filter_factory, CompactionFilter,
    CompactionFilterFactory, CompactionFilterHandle,
};
use comparator::{self, compare_callback, ComparatorCallback};
use crocksdb_ffi::{
    self, ChecksumType, DBBlockBasedTableOptions, DBBottommostLevelCompaction, DBCompactOptions,
    DBCompactionOptions, DBCompressionType, DBConcurrentTaskLimiter, DBFifoCompactionOptions,
    DBFlushOptions, DBHyperClockCacheOptions, DBInfoLogLevel, DBInstance, DBLRUCacheOptions,
    DBRateLimiter, DBRateLimiterMode, DBReadOptions, DBRecoveryMode, DBRestoreOptions, DBSnapshot,
    DBStatistics, DBStatisticsHistogramType, DBStatisticsTickerType, DBTitanDBOptions,
    DBTitanReadOptions, DBWriteBufferManager, DBWriteOptions, IndexType, Options,
    PrepopulateBlockCache,
};
use event_listener::{new_event_listener, EventListener};
use libc::{self, c_double, c_int, c_uchar, c_void, size_t};
use logger::{new_logger, Logger};
use merge_operator::MergeFn;
use merge_operator::{self, full_merge_callback, partial_merge_callback, MergeOperatorCallback};
use rocksdb::{Cache, Env, MemoryAllocator};
use slice_transform::{new_slice_transform, SliceTransform};
use sst_partitioner::{new_sst_partitioner_factory, SstPartitionerFactory};
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use table_filter::{destroy_table_filter, table_filter, TableFilter};
use table_properties_collector_factory::{
    new_table_properties_collector_factory, TablePropertiesCollectorFactory,
};
use titan::TitanDBOptions;
use TablePropertiesCollector;

#[derive(Default, Debug)]
pub struct HistogramData {
    pub median: f64,
    pub percentile95: f64,
    pub percentile99: f64,
    pub average: f64,
    pub standard_deviation: f64,
    pub max: f64,
}

pub struct BlockBasedOptions {
    inner: *mut DBBlockBasedTableOptions,
}

impl Drop for BlockBasedOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_destroy(self.inner);
        }
    }
}

impl Default for BlockBasedOptions {
    fn default() -> BlockBasedOptions {
        unsafe {
            let block_opts = crocksdb_ffi::crocksdb_block_based_options_create();
            assert!(
                !block_opts.is_null(),
                "Could not create rocksdb block based options"
            );
            BlockBasedOptions { inner: block_opts }
        }
    }
}

impl BlockBasedOptions {
    pub fn new() -> BlockBasedOptions {
        BlockBasedOptions::default()
    }

    pub fn set_metadata_block_size(&mut self, size: usize) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_metadata_block_size(self.inner, size);
        }
    }

    pub fn set_block_size(&mut self, size: usize) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_block_size(self.inner, size);
        }
    }

    pub fn set_index_type(&mut self, index_type: IndexType) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_index_type(self.inner, index_type);
        }
    }

    pub fn set_block_cache(&mut self, cache: &Cache) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_block_cache(self.inner, cache.inner);
        }
    }

    pub fn set_no_block_cache(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_no_block_cache(self.inner, v);
        }
    }

    pub fn set_bloom_filter(&mut self, bits_per_key: c_double, block_based: bool) {
        unsafe {
            let bloom = if block_based {
                crocksdb_ffi::crocksdb_filterpolicy_create_bloom(bits_per_key)
            } else {
                crocksdb_ffi::crocksdb_filterpolicy_create_bloom_full(bits_per_key)
            };

            crocksdb_ffi::crocksdb_block_based_options_set_filter_policy(self.inner, bloom);
        }
    }

    pub fn set_ribbon_filter(&mut self, bits_per_key: f64, bloom_before_level: i32) {
        unsafe {
            let filter =
                crocksdb_ffi::crocksdb_filterpolicy_create_ribbon(bits_per_key, bloom_before_level);
            crocksdb_ffi::crocksdb_block_based_options_set_filter_policy(self.inner, filter);
        }
    }

    pub fn set_optimize_filters_for_memory(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_optimize_filters_for_memory(
                self.inner, v as u8,
            );
        }
    }

    pub fn set_partition_filters(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_partition_filters(self.inner, v as u8);
        }
    }

    pub fn set_cache_index_and_filter_blocks(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_cache_index_and_filter_blocks(
                self.inner, v as u8,
            );
        }
    }

    pub fn set_pin_top_level_index_and_filter(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_pin_top_level_index_and_filter(
                self.inner, v as u8,
            );
        }
    }

    pub fn set_cache_index_and_filter_blocks_with_high_priority(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::
            crocksdb_block_based_options_set_cache_index_and_filter_blocks_with_high_priority(
                self.inner,
                v as u8,
            );
        }
    }

    pub fn set_whole_key_filtering(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_whole_key_filtering(self.inner, v);
        }
    }

    pub fn set_pin_l0_filter_and_index_blocks_in_cache(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_pin_l0_filter_and_index_blocks_in_cache(
                self.inner, v as u8,
            );
        }
    }

    pub fn set_read_amp_bytes_per_bit(&mut self, v: u32) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_read_amp_bytes_per_bit(
                self.inner, v as c_int,
            )
        }
    }

    pub fn set_format_version(&mut self, v: u32) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_format_version(self.inner, v as c_int);
        }
    }

    pub fn set_prepopulate_block_cache(&mut self, v: PrepopulateBlockCache) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_prepopulate_block_cache(self.inner, v);
        }
    }

    pub fn set_checksum(&mut self, v: ChecksumType) {
        unsafe {
            crocksdb_ffi::crocksdb_block_based_options_set_checksum(self.inner, v);
        }
    }
}

pub struct RateLimiter {
    inner: *mut DBRateLimiter,
}

unsafe impl Send for RateLimiter {}
unsafe impl Sync for RateLimiter {}

impl RateLimiter {
    pub fn new(rate_bytes_per_sec: i64, refill_period_us: i64, fairness: i32) -> RateLimiter {
        let limiter = unsafe {
            crocksdb_ffi::crocksdb_ratelimiter_create(
                rate_bytes_per_sec,
                refill_period_us,
                fairness,
            )
        };
        RateLimiter { inner: limiter }
    }

    pub fn new_with_auto_tuned(
        rate_bytes_per_sec: i64,
        refill_period_us: i64,
        fairness: i32,
        mode: DBRateLimiterMode,
        auto_tuned: bool,
    ) -> RateLimiter {
        let limiter = unsafe {
            crocksdb_ffi::crocksdb_ratelimiter_create_with_auto_tuned(
                rate_bytes_per_sec,
                refill_period_us,
                fairness,
                mode,
                auto_tuned,
            )
        };
        RateLimiter { inner: limiter }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_writeampbased_with_auto_tuned(
        rate_bytes_per_sec: i64,
        refill_period_us: i64,
        fairness: i32,
        mode: DBRateLimiterMode,
        auto_tuned: bool,
        tune_per_secs: i32,
        smooth_window_size: usize,
        recent_window_size: usize,
    ) -> RateLimiter {
        let limiter = unsafe {
            crocksdb_ffi::crocksdb_writeampbasedratelimiter_create_with_auto_tuned(
                rate_bytes_per_sec,
                refill_period_us,
                fairness,
                mode,
                auto_tuned,
                tune_per_secs,
                smooth_window_size,
                recent_window_size,
            )
        };
        RateLimiter { inner: limiter }
    }

    pub fn set_bytes_per_second(&self, bytes_per_sec: i64) {
        unsafe {
            crocksdb_ffi::crocksdb_ratelimiter_set_bytes_per_second(self.inner, bytes_per_sec);
        }
    }

    pub fn set_auto_tuned(&self, auto_tuned: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_ratelimiter_set_auto_tuned(self.inner, auto_tuned);
        }
    }

    pub fn get_singleburst_bytes(&self) -> i64 {
        unsafe { crocksdb_ffi::crocksdb_ratelimiter_get_singleburst_bytes(self.inner) }
    }

    pub fn request(&self, bytes: i64, pri: c_uchar) {
        unsafe {
            crocksdb_ffi::crocksdb_ratelimiter_request(self.inner, bytes, pri);
        }
    }

    pub fn get_total_bytes_through(&self, pri: c_uchar) -> i64 {
        unsafe { crocksdb_ffi::crocksdb_ratelimiter_get_total_bytes_through(self.inner, pri) }
    }

    pub fn get_bytes_per_second(&self) -> i64 {
        unsafe { crocksdb_ffi::crocksdb_ratelimiter_get_bytes_per_second(self.inner) }
    }

    pub fn get_total_requests(&self, pri: c_uchar) -> i64 {
        unsafe { crocksdb_ffi::crocksdb_ratelimiter_get_total_requests(self.inner, pri) }
    }

    pub fn get_auto_tuned(&self) -> bool {
        unsafe { crocksdb_ffi::crocksdb_ratelimiter_get_auto_tuned(self.inner) }
    }
}

impl Drop for RateLimiter {
    fn drop(&mut self) {
        unsafe { crocksdb_ffi::crocksdb_ratelimiter_destroy(self.inner) }
    }
}

pub struct Statistics {
    pub(crate) inner: *mut DBStatistics,
}

unsafe impl Send for Statistics {}
unsafe impl Sync for Statistics {}

impl Statistics {
    pub fn new() -> Self {
        unsafe {
            Self {
                inner: crocksdb_ffi::crocksdb_statistics_create(),
            }
        }
    }

    pub fn new_titan() -> Self {
        unsafe {
            Self {
                inner: crocksdb_ffi::crocksdb_titan_statistics_create(),
            }
        }
    }

    pub fn new_empty() -> Self {
        unsafe {
            Self {
                inner: crocksdb_ffi::crocksdb_empty_statistics_create(),
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        unsafe { crocksdb_ffi::crocksdb_statistics_is_empty(self.inner) }
    }

    pub fn to_string(&self) -> Option<String> {
        unsafe {
            let value = crocksdb_ffi::crocksdb_statistics_to_string(self.inner);

            if value.is_null() {
                return None;
            }

            // Must valid UTF-8 format.
            let s = CStr::from_ptr(value).to_str().unwrap().to_owned();
            libc::free(value as *mut c_void);
            Some(s)
        }
    }

    pub fn reset(&self) {
        unsafe {
            crocksdb_ffi::crocksdb_statistics_reset(self.inner);
        }
    }

    pub fn get_ticker_count(&self, ticker_type: DBStatisticsTickerType) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_statistics_get_ticker_count(self.inner, ticker_type) }
    }

    pub fn get_and_reset_ticker_count(&self, ticker_type: DBStatisticsTickerType) -> u64 {
        unsafe {
            crocksdb_ffi::crocksdb_statistics_get_and_reset_ticker_count(self.inner, ticker_type)
        }
    }

    pub fn get_histogram_string(&self, hist_type: DBStatisticsHistogramType) -> Option<String> {
        unsafe {
            let value =
                crocksdb_ffi::crocksdb_statistics_get_histogram_string(self.inner, hist_type);

            if value.is_null() {
                return None;
            }

            let s = CStr::from_ptr(value).to_str().unwrap().to_owned();
            libc::free(value as *mut c_void);
            Some(s)
        }
    }

    pub fn get_histogram(&self, hist_type: DBStatisticsHistogramType) -> Option<HistogramData> {
        unsafe {
            let mut data = HistogramData::default();
            let ret = crocksdb_ffi::crocksdb_statistics_get_histogram(
                self.inner,
                hist_type,
                &mut data.median,
                &mut data.percentile95,
                &mut data.percentile99,
                &mut data.average,
                &mut data.standard_deviation,
                &mut data.max,
            );
            if !ret {
                return None;
            }
            Some(data)
        }
    }
}

impl Drop for Statistics {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_statistics_destroy(self.inner);
        }
    }
}

pub struct WriteBufferManager {
    pub(crate) inner: *mut DBWriteBufferManager,
}

unsafe impl Send for WriteBufferManager {}
unsafe impl Sync for WriteBufferManager {}

impl WriteBufferManager {
    pub fn new(flush_size: usize, stall_ratio: f32, flush_oldest_first: bool) -> Self {
        unsafe {
            Self {
                inner: crocksdb_ffi::crocksdb_write_buffer_manager_create(
                    flush_size,
                    stall_ratio,
                    flush_oldest_first,
                ),
            }
        }
    }

    pub fn set_flush_size(&self, s: usize) {
        unsafe {
            crocksdb_ffi::crocksdb_write_buffer_manager_set_flush_size(self.inner, s);
        }
    }

    pub fn flush_size(&self) -> usize {
        unsafe { crocksdb_ffi::crocksdb_write_buffer_manager_flush_size(self.inner) }
    }

    pub fn set_flush_oldest_first(&self, f: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_write_buffer_manager_set_flush_oldest_first(self.inner, f);
        }
    }

    pub fn memory_usage(&self) -> usize {
        unsafe { crocksdb_ffi::crocksdb_write_buffer_manager_memory_usage(self.inner) }
    }
}

impl Drop for WriteBufferManager {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_write_buffer_manager_destroy(self.inner);
        }
    }
}

pub struct ConcurrentTaskLimiter {
    pub(crate) inner: *mut DBConcurrentTaskLimiter,
}

unsafe impl Send for ConcurrentTaskLimiter {}
unsafe impl Sync for ConcurrentTaskLimiter {}

impl ConcurrentTaskLimiter {
    pub fn new(name: &str, limit: u32) -> Self {
        let name = CString::new(name.as_bytes()).unwrap();
        unsafe {
            Self {
                inner: crocksdb_ffi::crocksdb_concurrent_task_limiter_create(name.as_ptr(), limit),
            }
        }
    }

    pub fn set_limit(&self, limit: u32) {
        unsafe {
            crocksdb_ffi::crocksdb_concurrent_task_limiter_set_limit(self.inner, limit);
        }
    }
}

impl Drop for ConcurrentTaskLimiter {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_concurrent_task_limiter_destroy(self.inner);
        }
    }
}

/// The UnsafeSnap must be destroyed by db, it maybe be leaked
/// if not using it properly, hence named as unsafe.
///
/// This object is convenient for wrapping snapshot by yourself. In most
/// cases, using `Snapshot` is enough.
pub struct UnsafeSnap {
    inner: *const DBSnapshot,
}

impl UnsafeSnap {
    pub unsafe fn new(db: *mut DBInstance) -> UnsafeSnap {
        UnsafeSnap {
            inner: crocksdb_ffi::crocksdb_create_snapshot(db),
        }
    }

    pub unsafe fn get_inner(&self) -> *const DBSnapshot {
        self.inner
    }

    /// Get the snapshot's sequence number.
    pub unsafe fn get_sequence_number(&self) -> u64 {
        crocksdb_ffi::crocksdb_get_snapshot_sequence_number(self.get_inner())
    }
}

pub struct ReadOptions {
    inner: *mut DBReadOptions,
    lower_bound: Vec<u8>,
    upper_bound: Vec<u8>,
    titan_inner: *mut DBTitanReadOptions,
}

impl Drop for ReadOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_destroy(self.inner);
            if !self.titan_inner.is_null() {
                crocksdb_ffi::ctitandb_readoptions_destroy(self.titan_inner);
            }
        }
    }
}

impl Default for ReadOptions {
    fn default() -> ReadOptions {
        unsafe {
            let opts = crocksdb_ffi::crocksdb_readoptions_create();
            assert!(!opts.is_null(), "Unable to create rocksdb read options");
            ReadOptions {
                inner: opts,
                lower_bound: vec![],
                upper_bound: vec![],
                titan_inner: ptr::null_mut::<DBTitanReadOptions>(),
            }
        }
    }
}

impl ReadOptions {
    pub fn new() -> ReadOptions {
        ReadOptions::default()
    }

    // TODO add snapshot setting here
    // TODO add snapshot wrapper structs with proper destructors;
    // that struct needs an "iterator" impl too.
    #[allow(dead_code)]

    pub fn set_verify_checksums(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_verify_checksums(self.inner, v);
        }
    }

    pub fn set_fill_cache(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_fill_cache(self.inner, v);
        }
    }

    pub fn set_auto_prefix_mode(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_auto_prefix_mode(self.inner, v);
        }
    }

    pub fn set_adaptive_readahead(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_adaptive_readahead(self.inner, v);
        }
    }

    pub unsafe fn set_snapshot(&mut self, snapshot: &UnsafeSnap) {
        crocksdb_ffi::crocksdb_readoptions_set_snapshot(self.inner, snapshot.inner);
    }

    pub fn set_iterate_lower_bound(&mut self, key: Vec<u8>) {
        self.lower_bound = key;
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_iterate_lower_bound(
                self.inner,
                self.lower_bound.as_ptr(),
                self.lower_bound.len(),
            );
        }
    }

    pub fn iterate_lower_bound(&self) -> &[u8] {
        &self.lower_bound
    }

    pub fn set_iterate_upper_bound(&mut self, key: Vec<u8>) {
        self.upper_bound = key;
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_iterate_upper_bound(
                self.inner,
                self.upper_bound.as_ptr(),
                self.upper_bound.len(),
            );
        }
    }

    pub fn iterate_upper_bound(&self) -> &[u8] {
        &self.upper_bound
    }

    pub fn set_read_tier(&mut self, tier: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_read_tier(self.inner, tier);
        }
    }

    pub fn set_tailing(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_tailing(self.inner, v);
        }
    }

    pub fn set_managed(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_managed(self.inner, v);
        }
    }

    pub fn set_readahead_size(&mut self, size: size_t) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_readahead_size(self.inner, size);
        }
    }

    pub fn set_max_skippable_internal_keys(&mut self, n: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_max_skippable_internal_keys(self.inner, n);
        }
    }

    pub fn set_total_order_seek(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_total_order_seek(self.inner, v);
        }
    }

    pub fn set_prefix_same_as_start(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_prefix_same_as_start(self.inner, v);
        }
    }

    pub fn set_pin_data(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_pin_data(self.inner, v);
        }
    }

    pub fn set_background_purge_on_iterator_cleanup(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_background_purge_on_iterator_cleanup(
                self.inner, v,
            );
        }
    }

    pub fn set_ignore_range_deletions(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_readoptions_set_ignore_range_deletions(self.inner, v);
        }
    }

    pub fn get_inner(&self) -> *const DBReadOptions {
        self.inner
    }

    pub fn get_titan_inner(&self) -> *const DBTitanReadOptions {
        self.titan_inner
    }

    pub fn set_titan_key_only(&mut self, v: bool) {
        unsafe {
            if self.titan_inner.is_null() {
                self.titan_inner = crocksdb_ffi::ctitandb_readoptions_create();
            }
            crocksdb_ffi::ctitandb_readoptions_set_key_only(self.titan_inner, v);
        }
    }

    pub fn set_table_filter<T: TableFilter>(&mut self, filter: T) {
        unsafe {
            let f = Box::into_raw(Box::new(filter));
            let f = f as *mut c_void;
            crocksdb_ffi::crocksdb_readoptions_set_table_filter(
                self.inner,
                f,
                table_filter::<T>,
                destroy_table_filter::<T>,
            );
        }
    }
}

pub struct WriteOptions {
    pub(crate) inner: *mut DBWriteOptions,
}

impl Drop for WriteOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_writeoptions_destroy(self.inner);
        }
    }
}

impl Default for WriteOptions {
    fn default() -> WriteOptions {
        let write_opts = unsafe { crocksdb_ffi::crocksdb_writeoptions_create() };
        assert!(
            !write_opts.is_null(),
            "Could not create rocksdb write options"
        );
        WriteOptions { inner: write_opts }
    }
}

impl WriteOptions {
    pub fn new() -> WriteOptions {
        WriteOptions::default()
    }

    pub fn set_sync(&mut self, sync: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_writeoptions_set_sync(self.inner, sync);
        }
    }

    pub fn disable_wal(&mut self, disable: bool) {
        unsafe {
            if disable {
                crocksdb_ffi::crocksdb_writeoptions_disable_wal(self.inner, 1);
            } else {
                crocksdb_ffi::crocksdb_writeoptions_disable_wal(self.inner, 0);
            }
        }
    }

    pub fn set_ignore_missing_column_families(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_writeoptions_set_ignore_missing_column_families(self.inner, v);
        }
    }

    pub fn set_no_slowdown(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_writeoptions_set_no_slowdown(self.inner, v);
        }
    }

    pub fn set_low_pri(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_writeoptions_set_low_pri(self.inner, v);
        }
    }

    pub fn set_memtable_insert_hint_per_batch(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_writeoptions_set_memtable_insert_hint_per_batch(self.inner, v);
        }
    }
}

pub struct CompactOptions {
    pub(crate) inner: *mut DBCompactOptions,
}

impl CompactOptions {
    pub fn new() -> CompactOptions {
        unsafe {
            CompactOptions {
                inner: crocksdb_ffi::crocksdb_compactoptions_create(),
            }
        }
    }

    pub fn set_exclusive_manual_compaction(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_compactoptions_set_exclusive_manual_compaction(self.inner, v);
        }
    }

    pub fn set_change_level(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_compactoptions_set_change_level(self.inner, v);
        }
    }

    pub fn set_target_level(&mut self, v: i32) {
        unsafe {
            crocksdb_ffi::crocksdb_compactoptions_set_target_level(self.inner, v);
        }
    }

    pub fn set_target_path_id(&mut self, v: i32) {
        unsafe {
            crocksdb_ffi::crocksdb_compactoptions_set_target_path_id(self.inner, v);
        }
    }

    pub fn set_max_subcompactions(&mut self, v: i32) {
        unsafe {
            crocksdb_ffi::crocksdb_compactoptions_set_max_subcompactions(self.inner, v);
        }
    }

    pub fn set_bottommost_level_compaction(&mut self, v: DBBottommostLevelCompaction) {
        unsafe {
            crocksdb_ffi::crocksdb_compactoptions_set_bottommost_level_compaction(self.inner, v);
        }
    }
}

impl Drop for CompactOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_compactoptions_destroy(self.inner);
        }
    }
}

pub struct CompactionOptions {
    pub(crate) inner: *mut DBCompactionOptions,
}

impl CompactionOptions {
    pub fn new() -> CompactionOptions {
        unsafe {
            CompactionOptions {
                inner: crocksdb_ffi::crocksdb_compaction_options_create(),
            }
        }
    }

    pub fn set_compression(&mut self, compression: DBCompressionType) {
        unsafe {
            crocksdb_ffi::crocksdb_compaction_options_set_compression(self.inner, compression);
        }
    }

    pub fn set_output_file_size_limit(&mut self, size: usize) {
        unsafe {
            crocksdb_ffi::crocksdb_compaction_options_set_output_file_size_limit(self.inner, size);
        }
    }

    pub fn set_max_subcompactions(&mut self, v: i32) {
        unsafe {
            crocksdb_ffi::crocksdb_compaction_options_set_max_subcompactions(self.inner, v);
        }
    }
}

impl Drop for CompactionOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_compaction_options_destroy(self.inner);
        }
    }
}

pub struct DBOptions {
    pub(crate) inner: *mut Options,
    env: Option<Arc<Env>>,
    pub(crate) titan_inner: *mut DBTitanDBOptions,
}

impl Drop for DBOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_options_destroy(self.inner);
            if !self.titan_inner.is_null() {
                crocksdb_ffi::ctitandb_options_destroy(self.titan_inner);
            }
        }
    }
}

impl Default for DBOptions {
    fn default() -> DBOptions {
        unsafe {
            let opts = crocksdb_ffi::crocksdb_options_create();
            assert!(!opts.is_null(), "Could not create rocksdb db options");
            DBOptions {
                inner: opts,
                env: None,
                titan_inner: ptr::null_mut::<DBTitanDBOptions>(),
            }
        }
    }
}

impl Clone for DBOptions {
    fn clone(&self) -> Self {
        unsafe {
            let opts = crocksdb_ffi::crocksdb_options_copy(self.inner);
            assert!(!opts.is_null());
            let mut titan_opts = ptr::null_mut::<DBTitanDBOptions>();
            if !self.titan_inner.is_null() {
                titan_opts = crocksdb_ffi::ctitandb_options_copy(self.titan_inner);
            }
            DBOptions {
                inner: opts,
                env: self.env.clone(),
                titan_inner: titan_opts,
            }
        }
    }
}

impl DBOptions {
    pub fn new() -> DBOptions {
        DBOptions::default()
    }

    pub fn env(&self) -> Option<Arc<Env>> {
        self.env.clone()
    }

    pub unsafe fn from_raw(inner: *mut Options) -> DBOptions {
        DBOptions {
            inner,
            env: None,
            titan_inner: ptr::null_mut::<DBTitanDBOptions>(),
        }
    }

    pub fn set_titandb_options(&mut self, opts: &TitanDBOptions) {
        unsafe {
            self.titan_inner = crocksdb_ffi::ctitandb_options_copy(opts.inner);
        }
    }

    pub fn increase_parallelism(&mut self, parallelism: i32) {
        unsafe {
            crocksdb_ffi::crocksdb_options_increase_parallelism(self.inner, parallelism);
        }
    }

    pub fn add_event_listener<L: EventListener>(&mut self, l: L) {
        let handle = new_event_listener(l);
        unsafe { crocksdb_ffi::crocksdb_options_add_eventlistener(self.inner, handle) }
    }

    pub fn create_if_missing(&mut self, create_if_missing: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_create_if_missing(self.inner, create_if_missing);
        }
    }

    pub fn create_missing_column_families(&mut self, create_missing_column_families: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_create_missing_column_families(
                self.inner,
                create_missing_column_families,
            )
        }
    }

    pub fn set_env(&mut self, env: Arc<Env>) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_env(self.inner, env.inner);
            self.env = Some(env);
        }
    }

    pub fn set_max_open_files(&mut self, nfiles: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_open_files(self.inner, nfiles);
        }
    }

    pub fn set_max_total_wal_size(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_total_wal_size(self.inner, size);
        }
    }

    pub fn set_use_fsync(&mut self, useit: bool) {
        unsafe {
            if useit {
                crocksdb_ffi::crocksdb_options_set_use_fsync(self.inner, 1)
            } else {
                crocksdb_ffi::crocksdb_options_set_use_fsync(self.inner, 0)
            }
        }
    }

    pub fn set_bytes_per_sync(&mut self, nbytes: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_bytes_per_sync(self.inner, nbytes);
        }
    }

    pub fn set_table_cache_num_shard_bits(&mut self, nbits: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_table_cache_numshardbits(self.inner, nbits);
        }
    }

    pub fn set_writable_file_max_buffer_size(&mut self, nbytes: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_writable_file_max_buffer_size(self.inner, nbytes);
        }
    }

    pub fn set_use_direct_reads(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_use_direct_reads(self.inner, v);
        }
    }

    pub fn set_use_direct_io_for_flush_and_compaction(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_use_direct_io_for_flush_and_compaction(
                self.inner, v,
            );
        }
    }

    pub fn set_max_manifest_file_size(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_manifest_file_size(self.inner, size);
        }
    }

    pub fn set_max_background_jobs(&mut self, n: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_background_jobs(self.inner, n);
        }
    }

    pub fn get_max_background_jobs(&self) -> i32 {
        unsafe { crocksdb_ffi::crocksdb_options_get_max_background_jobs(self.inner) }
    }

    pub fn set_max_background_compactions(&mut self, n: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_background_compactions(self.inner, n);
        }
    }

    pub fn get_max_background_compactions(&self) -> i32 {
        unsafe { crocksdb_ffi::crocksdb_options_get_max_background_compactions(self.inner) }
    }

    pub fn set_max_background_flushes(&mut self, n: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_background_flushes(self.inner, n);
        }
    }

    pub fn get_max_background_flushes(&self) -> i32 {
        unsafe { crocksdb_ffi::crocksdb_options_get_max_background_flushes(self.inner) }
    }

    pub fn set_max_subcompactions(&mut self, n: u32) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_subcompactions(self.inner, n);
        }
    }

    pub fn set_wal_bytes_per_sync(&mut self, n: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_wal_bytes_per_sync(self.inner, n);
        }
    }

    pub fn set_wal_recovery_mode(&mut self, mode: DBRecoveryMode) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_wal_recovery_mode(self.inner, mode);
        }
    }

    pub fn set_delayed_write_rate(&mut self, rate: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_delayed_write_rate(self.inner, rate);
        }
    }

    pub fn set_write_buffer_manager(&mut self, wbm: &WriteBufferManager) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_write_buffer_manager(self.inner, wbm.inner);
        }
    }

    pub fn set_statistics(&mut self, s: &Statistics) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_statistics(self.inner, s.inner);
        }
    }

    pub fn set_stats_dump_period_sec(&mut self, period: usize) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_stats_dump_period_sec(self.inner, period);
        }
    }

    pub fn set_stats_persist_period_sec(&mut self, n: u32) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_stats_persist_period_sec(self.inner, n);
        }
    }

    pub fn set_db_log_dir(&mut self, path: &str) {
        let path = CString::new(path.as_bytes()).unwrap();
        unsafe {
            crocksdb_ffi::crocksdb_options_set_db_log_dir(self.inner, path.as_ptr());
        }
    }

    pub fn set_wal_dir(&mut self, path: &str) {
        let path = CString::new(path.as_bytes()).unwrap();
        unsafe {
            crocksdb_ffi::crocksdb_options_set_wal_dir(self.inner, path.as_ptr());
        }
    }

    pub fn set_wal_ttl_seconds(&mut self, ttl: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_wal_ttl_seconds(self.inner, ttl);
        }
    }

    pub fn set_wal_size_limit_mb(&mut self, limit: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_wal_size_limit_mb(self.inner, limit);
        }
    }

    pub fn set_max_log_file_size(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_log_file_size(self.inner, size as size_t);
        }
    }

    pub fn set_log_file_time_to_roll(&mut self, ttl: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_log_file_time_to_roll(self.inner, ttl as size_t);
        }
    }

    pub fn set_info_log_level(&mut self, level: DBInfoLogLevel) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_info_log_level(self.inner, level);
        }
    }

    pub fn set_keep_log_file_num(&mut self, num: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_keep_log_file_num(self.inner, num as size_t);
        }
    }

    pub fn set_recycle_log_file_num(&mut self, num: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_recycle_log_file_num(self.inner, num as size_t);
        }
    }

    pub fn set_compaction_readahead_size(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_compaction_readahead_size(
                self.inner,
                size as size_t,
            );
        }
    }

    pub fn set_rate_limiter(&mut self, rate_limiter: &RateLimiter) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_ratelimiter(self.inner, rate_limiter.inner);
        }
    }

    pub fn get_rate_limiter(&self) -> Option<RateLimiter> {
        let limiter = unsafe { crocksdb_ffi::crocksdb_options_get_ratelimiter(self.inner) };
        if limiter.is_null() {
            None
        } else {
            Some(RateLimiter { inner: limiter })
        }
    }

    pub fn get_write_buffer_manager(&self) -> Option<WriteBufferManager> {
        let manager =
            unsafe { crocksdb_ffi::crocksdb_options_get_write_buffer_manager(self.inner) };
        if manager.is_null() {
            None
        } else {
            Some(WriteBufferManager { inner: manager })
        }
    }

    // Create a info log with `path` and save to options logger field directly.
    // TODO: export more logger options like level, roll size, time, etc...
    pub fn create_info_log(&self, path: &str) -> Result<(), String> {
        let cpath = match CString::new(path.as_bytes()) {
            Ok(c) => c,
            Err(_) => {
                return Err(
                    "Failed to convert path to CString when creating rocksdb info log".to_owned(),
                );
            }
        };

        unsafe {
            let logger = ffi_try!(crocksdb_create_log_from_options(cpath.as_ptr(), self.inner));
            crocksdb_ffi::crocksdb_options_set_info_log(self.inner, logger);
            // logger uses shared_ptr, it is OK to destroy here.
            crocksdb_ffi::crocksdb_log_destroy(logger);
        }

        Ok(())
    }

    // Set the logger to options.
    pub fn set_info_log<L: Logger>(&self, l: L) {
        let logger = new_logger(l);
        unsafe {
            crocksdb_ffi::crocksdb_options_set_info_log(self.inner, logger);
        }
    }

    pub fn enable_pipelined_write(&self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_enable_pipelined_write(self.inner, v);
        }
    }

    pub fn enable_multi_batch_write(&self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_enable_multi_batch_write(self.inner, v);
        }
    }

    pub fn is_enable_multi_batch_write(&self) -> bool {
        unsafe { crocksdb_ffi::crocksdb_options_is_enable_multi_batch_write(self.inner) }
    }

    pub fn enable_unordered_write(&self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_unordered_write(self.inner, v);
        }
    }

    pub fn allow_concurrent_memtable_write(&self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_allow_concurrent_memtable_write(self.inner, v);
        }
    }

    pub fn manual_wal_flush(&self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_manual_wal_flush(self.inner, v);
        }
    }

    /// the second parameter is a slice which contains tuples (path, target_size).
    pub fn set_db_paths<T: AsRef<Path>>(&self, val: &[(T, u64)]) {
        let num_paths = val.len();
        let mut cpaths = Vec::with_capacity(num_paths);
        let mut cpath_lens = Vec::with_capacity(num_paths);
        let mut sizes = Vec::with_capacity(num_paths);
        for dbpath in val {
            let dbpath_str = dbpath.0.as_ref().to_str();
            cpaths.push(dbpath_str.unwrap().as_ptr() as _);
            cpath_lens.push(dbpath_str.unwrap().len());
            sizes.push(dbpath.1);
        }

        unsafe {
            crocksdb_ffi::crocksdb_options_set_db_paths(
                self.inner,
                cpaths.as_ptr(),
                cpath_lens.as_ptr(),
                sizes.as_ptr(),
                num_paths as c_int,
            );
        }
    }

    pub fn set_atomic_flush(&self, enable: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_atomic_flush(self.inner, enable);
        }
    }

    pub fn avoid_flush_during_recovery(&self, avoid: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_avoid_flush_during_recovery(self.inner, avoid);
        }
    }

    pub fn avoid_flush_during_shutdown(&self, avoid: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_avoid_flush_during_shutdown(self.inner, avoid);
        }
    }

    pub fn get_db_paths_num(&self) -> usize {
        unsafe { crocksdb_ffi::crocksdb_options_get_db_paths_num(self.inner) }
    }

    pub fn get_db_path(&self, idx: usize) -> Option<String> {
        unsafe {
            let ptr = crocksdb_ffi::crocksdb_options_get_db_path(self.inner, idx as size_t);
            if ptr.is_null() {
                return None;
            }
            let s = CStr::from_ptr(ptr).to_str().unwrap().to_owned();
            Some(s)
        }
    }

    pub fn get_path_target_size(&self, idx: usize) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_path_target_size(self.inner, idx as size_t) }
    }

    /// Set paranoid checks. The default value is `true`. We can set it to `false`
    /// to skip manifest checks.
    pub fn set_paranoid_checks(&self, enable: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_paranoid_checks(self.inner, enable as u8);
        }
    }

    pub fn set_doubly_skiplist(&self) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_doubly_skip_list_rep(self.inner);
        }
    }

    pub fn get_memtable_name(&self) -> Option<&str> {
        unsafe {
            let memtable_name =
                crocksdb_ffi::crocksdb_options_get_memtable_factory_name(self.inner);
            if memtable_name.is_null() {
                return None;
            }
            Some(CStr::from_ptr(memtable_name).to_str().unwrap())
        }
    }

    pub fn set_track_and_verify_wals_in_manifest(&self, enable: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_track_and_verify_wals_in_manifest(
                self.inner, enable,
            );
        }
    }
}

pub struct ColumnFamilyOptions {
    pub(crate) inner: *mut Options,
    pub(crate) titan_inner: *mut DBTitanDBOptions,
    env: Option<Arc<Env>>,
    filter: Option<CompactionFilterHandle>,
}

impl Drop for ColumnFamilyOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_options_destroy(self.inner);
            if !self.titan_inner.is_null() {
                crocksdb_ffi::ctitandb_options_destroy(self.titan_inner);
            }
        }
    }
}

impl Default for ColumnFamilyOptions {
    fn default() -> ColumnFamilyOptions {
        unsafe {
            let opts = crocksdb_ffi::crocksdb_options_create();
            assert!(
                !opts.is_null(),
                "Could not create rocksdb column family options"
            );
            ColumnFamilyOptions {
                inner: opts,
                titan_inner: ptr::null_mut::<DBTitanDBOptions>(),
                env: None,
                filter: None,
            }
        }
    }
}

impl Clone for ColumnFamilyOptions {
    fn clone(&self) -> Self {
        assert!(self.filter.is_none());
        unsafe {
            let opts = crocksdb_ffi::crocksdb_options_copy(self.inner);
            assert!(!opts.is_null());
            let mut titan_opts = ptr::null_mut::<DBTitanDBOptions>();
            if !self.titan_inner.is_null() {
                titan_opts = crocksdb_ffi::ctitandb_options_copy(self.titan_inner);
            }
            ColumnFamilyOptions {
                inner: opts,
                titan_inner: titan_opts,
                env: self.env.clone(),
                filter: None,
            }
        }
    }
}

impl ColumnFamilyOptions {
    pub fn new() -> ColumnFamilyOptions {
        ColumnFamilyOptions::default()
    }

    pub unsafe fn from_raw(
        inner: *mut Options,
        titan_inner: *mut DBTitanDBOptions,
    ) -> ColumnFamilyOptions {
        assert!(
            !inner.is_null(),
            "could not new rocksdb options with null inner"
        );
        ColumnFamilyOptions {
            inner,
            titan_inner,
            env: None,
            filter: None,
        }
    }

    pub fn set_titandb_options(&mut self, opts: &TitanDBOptions) {
        unsafe {
            self.titan_inner = crocksdb_ffi::ctitandb_options_copy(opts.inner);
        }
    }

    pub fn optimize_level_style_compaction(&mut self, memtable_memory_budget: i32) {
        unsafe {
            crocksdb_ffi::crocksdb_options_optimize_level_style_compaction(
                self.inner,
                memtable_memory_budget,
            );
        }
    }

    pub fn set_env(&mut self, env: Arc<Env>) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_env(self.inner, env.inner);
            self.env = Some(env);
        }
    }

    /// Set compaction filter.
    ///
    /// filter will be dropped when this option is dropped or a new filter is
    /// set.
    ///
    /// By default, compaction will only pass keys written after the most
    /// recent call to GetSnapshot() to filter.
    ///
    /// See also `CompactionFilter`.
    pub fn set_compaction_filter<S, C>(&mut self, name: S, filter: C) -> Result<(), String>
    where
        S: Into<Vec<u8>>,
        C: CompactionFilter,
    {
        unsafe {
            let c_name = match CString::new(name) {
                Ok(s) => s,
                Err(e) => return Err(format!("failed to convert to cstring: {:?}", e)),
            };
            let filter = new_compaction_filter::<C>(c_name, filter);
            crocksdb_ffi::crocksdb_options_set_compaction_filter(self.inner, filter.inner);
            self.filter = Some(filter);
            Ok(())
        }
    }

    /// Set compaction filter factory.
    ///
    /// See also `CompactionFilterFactory`.
    pub fn set_compaction_filter_factory<S, C>(&mut self, name: S, factory: C) -> Result<(), String>
    where
        S: Into<Vec<u8>>,
        C: CompactionFilterFactory,
    {
        let c_name = match CString::new(name) {
            Ok(s) => s,
            Err(e) => return Err(format!("failed to convert to cstring: {:?}", e)),
        };
        unsafe {
            let factory = new_compaction_filter_factory::<C>(c_name, factory)?;
            crocksdb_ffi::crocksdb_options_set_compaction_filter_factory(self.inner, factory.inner);
            std::mem::forget(factory); // Deconstructor will be called after `self` is dropped.
            Ok(())
        }
    }

    pub fn set_compaction_thread_limiter(&mut self, limiter: &ConcurrentTaskLimiter) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_compaction_thread_limiter(self.inner, limiter.inner);
        }
    }

    pub fn get_compaction_thread_limiter(&self) -> Option<ConcurrentTaskLimiter> {
        let limiter =
            unsafe { crocksdb_ffi::crocksdb_options_get_compaction_thread_limiter(self.inner) };
        if limiter.is_null() {
            None
        } else {
            Some(ConcurrentTaskLimiter { inner: limiter })
        }
    }

    pub fn add_table_properties_collector_factory<
        C: TablePropertiesCollector,
        T: TablePropertiesCollectorFactory<C>,
    >(
        &mut self,
        fname: &str,
        factory: T,
    ) {
        unsafe {
            let f = new_table_properties_collector_factory(fname, factory);
            crocksdb_ffi::crocksdb_options_add_table_properties_collector_factory(self.inner, f);
        }
    }

    pub fn compression(&mut self, t: DBCompressionType) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_compression(self.inner, t);
        }
    }

    pub fn get_compression(&self) -> DBCompressionType {
        unsafe { crocksdb_ffi::crocksdb_options_get_compression(self.inner) }
    }

    pub fn set_compression_options(
        &mut self,
        window_bits: i32,
        level: i32,
        strategy: i32,
        max_dict_bytes: i32,
        zstd_max_train_bytes: i32,
        parallel_threads: i32,
    ) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_compression_options(
                self.inner,
                window_bits,
                level,
                strategy,
                max_dict_bytes,
                zstd_max_train_bytes,
                parallel_threads,
            )
        }
    }

    pub fn set_bottommost_level_compression_options(
        &mut self,
        window_bits: i32,
        level: i32,
        strategy: i32,
        max_dict_bytes: i32,
        zstd_max_train_bytes: i32,
        parallel_threads: i32,
    ) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_bottommost_compression_options(
                self.inner,
                window_bits,
                level,
                strategy,
                max_dict_bytes,
                zstd_max_train_bytes,
                parallel_threads,
            )
        }
    }

    pub fn compression_per_level(&mut self, level_types: &[DBCompressionType]) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_compression_per_level(
                self.inner,
                level_types.as_ptr(),
                level_types.len() as size_t,
            )
        }
    }

    pub fn get_compression_per_level(&self) -> Vec<DBCompressionType> {
        unsafe {
            let size = crocksdb_ffi::crocksdb_options_get_compression_level_number(self.inner);
            let mut ret = Vec::with_capacity(size);
            let pret = ret.as_mut_ptr();
            crocksdb_ffi::crocksdb_options_get_compression_per_level(self.inner, pret);
            ret.set_len(size);
            ret
        }
    }

    pub fn bottommost_compression(&self, c: DBCompressionType) {
        unsafe { crocksdb_ffi::crocksdb_set_bottommost_compression(self.inner, c) }
    }

    pub fn add_merge_operator(&mut self, name: &str, merge_fn: MergeFn) {
        let cb = Box::new(MergeOperatorCallback {
            name: CString::new(name.as_bytes()).unwrap(),
            merge_fn,
        });
        let cb = Box::into_raw(cb) as *mut c_void;

        unsafe {
            let mo = crocksdb_ffi::crocksdb_mergeoperator_create(
                cb,
                merge_operator::destructor_callback,
                full_merge_callback,
                partial_merge_callback,
                None,
                merge_operator::name_callback,
            );
            crocksdb_ffi::crocksdb_options_set_merge_operator(self.inner, mo);
        }
    }

    pub fn add_comparator(&mut self, name: &str, compare_fn: fn(&[u8], &[u8]) -> i32) {
        let cb = Box::new(ComparatorCallback {
            name: CString::new(name.as_bytes()).unwrap(),
            f: compare_fn,
        });
        let cb = Box::into_raw(cb) as *mut c_void;

        unsafe {
            let cmp = crocksdb_ffi::crocksdb_comparator_create(
                cb,
                comparator::destructor_callback,
                compare_callback,
                comparator::name_callback,
            );
            crocksdb_ffi::crocksdb_options_set_comparator(self.inner, cmp);
        }
    }

    pub fn set_block_cache_size_mb(&mut self, cache_size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_optimize_for_point_lookup(self.inner, cache_size);
        }
    }

    pub fn set_min_write_buffer_number(&mut self, nbuf: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_min_write_buffer_number_to_merge(self.inner, nbuf);
        }
    }

    pub fn get_min_write_buffer_number(&self) -> u32 {
        unsafe {
            crocksdb_ffi::crocksdb_options_get_min_write_buffer_number_to_merge(self.inner) as u32
        }
    }

    pub fn set_max_write_buffer_number(&mut self, nbuf: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_write_buffer_number(self.inner, nbuf);
        }
    }

    pub fn get_max_write_buffer_number(&self) -> u32 {
        unsafe { crocksdb_ffi::crocksdb_options_get_max_write_buffer_number(self.inner) as u32 }
    }

    pub fn set_write_buffer_size(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_write_buffer_size(self.inner, size);
        }
    }

    pub fn get_write_buffer_size(&self) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_write_buffer_size(self.inner) }
    }

    pub fn set_max_bytes_for_level_base(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_bytes_for_level_base(self.inner, size);
        }
    }

    pub fn get_max_bytes_for_level_base(&self) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_max_bytes_for_level_base(self.inner) }
    }

    pub fn set_max_bytes_for_level_multiplier(&mut self, mul: i32) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_bytes_for_level_multiplier(
                self.inner,
                f64::from(mul),
            );
        }
    }

    pub fn get_max_bytes_for_level_multiplier(&self) -> i32 {
        unsafe {
            crocksdb_ffi::crocksdb_options_get_max_bytes_for_level_multiplier(self.inner) as i32
        }
    }

    pub fn set_max_compaction_bytes(&mut self, bytes: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_max_compaction_bytes(self.inner, bytes);
        }
    }

    pub fn get_max_compaction_bytes(&self) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_max_compaction_bytes(self.inner) }
    }

    pub fn set_level_compaction_dynamic_level_bytes(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_level_compaction_dynamic_level_bytes(self.inner, v);
        }
    }

    pub fn get_level_compaction_dynamic_level_bytes(&self) -> bool {
        unsafe {
            crocksdb_ffi::crocksdb_options_get_level_compaction_dynamic_level_bytes(self.inner)
        }
    }

    pub fn set_soft_pending_compaction_bytes_limit(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_soft_pending_compaction_bytes_limit(
                self.inner, size,
            );
        }
    }

    pub fn get_soft_pending_compaction_bytes_limit(&self) -> u64 {
        unsafe {
            crocksdb_ffi::crocksdb_options_get_soft_pending_compaction_bytes_limit(self.inner)
        }
    }

    pub fn set_hard_pending_compaction_bytes_limit(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_hard_pending_compaction_bytes_limit(
                self.inner, size,
            );
        }
    }

    pub fn get_hard_pending_compaction_bytes_limit(&self) -> u64 {
        unsafe {
            crocksdb_ffi::crocksdb_options_get_hard_pending_compaction_bytes_limit(self.inner)
        }
    }

    pub fn set_target_file_size_base(&mut self, size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_target_file_size_base(self.inner, size);
        }
    }

    pub fn set_target_file_size_multiplier(&mut self, multiplier: i32) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_target_file_size_multiplier(self.inner, multiplier)
        }
    }

    pub fn get_target_file_size_base(&self) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_target_file_size_base(self.inner) }
    }

    pub fn set_min_write_buffer_number_to_merge(&mut self, to_merge: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_min_write_buffer_number_to_merge(
                self.inner, to_merge,
            );
        }
    }

    pub fn get_min_write_buffer_number_to_merge(&self) -> u32 {
        unsafe {
            crocksdb_ffi::crocksdb_options_get_min_write_buffer_number_to_merge(self.inner) as u32
        }
    }

    pub fn set_level_zero_file_num_compaction_trigger(&mut self, n: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_level0_file_num_compaction_trigger(self.inner, n);
        }
    }

    pub fn get_level_zero_file_num_compaction_trigger(&self) -> u32 {
        unsafe {
            crocksdb_ffi::crocksdb_options_get_level0_file_num_compaction_trigger(self.inner) as u32
        }
    }

    pub fn set_level_zero_slowdown_writes_trigger(&mut self, n: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_level0_slowdown_writes_trigger(self.inner, n);
        }
    }

    pub fn get_level_zero_slowdown_writes_trigger(&self) -> u32 {
        unsafe {
            crocksdb_ffi::crocksdb_options_get_level0_slowdown_writes_trigger(self.inner) as u32
        }
    }

    pub fn set_level_zero_stop_writes_trigger(&mut self, n: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_level0_stop_writes_trigger(self.inner, n);
        }
    }

    pub fn get_level_zero_stop_writes_trigger(&self) -> u32 {
        unsafe { crocksdb_ffi::crocksdb_options_get_level0_stop_writes_trigger(self.inner) as u32 }
    }

    pub fn set_compaction_style(&mut self, style: crocksdb_ffi::DBCompactionStyle) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_compaction_style(self.inner, style);
        }
    }

    pub fn compaction_priority(&mut self, priority: crocksdb_ffi::CompactionPriority) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_compaction_priority(self.inner, priority);
        }
    }

    pub fn set_disable_auto_compactions(&mut self, disable: bool) {
        unsafe {
            if disable {
                crocksdb_ffi::crocksdb_options_set_disable_auto_compactions(self.inner, 1)
            } else {
                crocksdb_ffi::crocksdb_options_set_disable_auto_compactions(self.inner, 0)
            }
        }
    }

    pub fn get_disable_auto_compactions(&self) -> bool {
        unsafe { crocksdb_ffi::crocksdb_options_get_disable_auto_compactions(self.inner) == 1 }
    }

    pub fn set_disable_write_stall(&mut self, disable: bool) {
        unsafe { crocksdb_ffi::crocksdb_options_set_disable_write_stall(self.inner, disable) }
    }

    pub fn get_disable_write_stall(&self) -> bool {
        unsafe { crocksdb_ffi::crocksdb_options_get_disable_write_stall(self.inner) }
    }

    pub fn set_block_based_table_factory(&mut self, factory: &BlockBasedOptions) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_block_based_table_factory(self.inner, factory.inner);
        }
    }

    pub fn set_report_bg_io_stats(&mut self, enable: bool) {
        unsafe {
            if enable {
                crocksdb_ffi::crocksdb_options_set_report_bg_io_stats(self.inner, 1);
            } else {
                crocksdb_ffi::crocksdb_options_set_report_bg_io_stats(self.inner, 0);
            }
        }
    }

    pub fn set_num_levels(&mut self, n: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_num_levels(self.inner, n);
        }
    }

    pub fn get_num_levels(&self) -> usize {
        unsafe { crocksdb_ffi::crocksdb_options_get_num_levels(self.inner) as usize }
    }

    pub fn set_prefix_extractor<S, ST>(&mut self, name: S, transform: ST) -> Result<(), String>
    where
        S: Into<Vec<u8>>,
        ST: SliceTransform,
    {
        unsafe {
            let c_name = match CString::new(name) {
                Ok(s) => s,
                Err(e) => return Err(format!("failed to convert to cstring: {:?}", e)),
            };
            let transform = new_slice_transform::<ST>(c_name, transform)?;
            crocksdb_ffi::crocksdb_options_set_prefix_extractor(self.inner, transform);
            Ok(())
        }
    }

    pub fn set_optimize_filters_for_hits(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_optimize_filters_for_hits(self.inner, v);
        }
    }

    pub fn set_memtable_insert_hint_prefix_extractor<S, ST>(
        &mut self,
        name: S,
        transform: ST,
    ) -> Result<(), String>
    where
        S: Into<Vec<u8>>,
        ST: SliceTransform,
    {
        unsafe {
            let c_name = match CString::new(name) {
                Ok(s) => s,
                Err(e) => return Err(format!("failed to convert to cstring: {:?}", e)),
            };
            let transform = new_slice_transform::<ST>(c_name, transform)?;
            crocksdb_ffi::crocksdb_options_set_memtable_insert_with_hint_prefix_extractor(
                self.inner, transform,
            );
            Ok(())
        }
    }

    pub fn set_memtable_prefix_bloom_size_ratio(&mut self, ratio: f64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_memtable_prefix_bloom_size_ratio(self.inner, ratio);
        }
    }

    pub fn set_force_consistency_checks(&mut self, v: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_force_consistency_checks(self.inner, v);
        }
    }

    pub fn get_force_consistency_checks(&self) -> bool {
        unsafe { crocksdb_ffi::crocksdb_options_get_force_consistency_checks(self.inner) }
    }

    pub fn get_block_cache_usage(&self) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_block_cache_usage(self.inner) as u64 }
    }

    pub fn get_blob_cache_usage(&self) -> u64 {
        unsafe { crocksdb_ffi::ctitandb_options_get_blob_cache_usage(self.titan_inner) as u64 }
    }

    pub fn set_block_cache_capacity(&self, capacity: u64) -> Result<(), String> {
        unsafe {
            ffi_try!(crocksdb_options_set_block_cache_capacity(
                self.inner,
                capacity as usize
            ));
            Ok(())
        }
    }

    pub fn get_block_cache_capacity(&self) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_block_cache_capacity(self.inner) as u64 }
    }

    pub fn set_blob_cache_capacity(&self, capacity: u64) -> Result<(), String> {
        unsafe {
            ffi_try!(ctitandb_options_set_blob_cache_capacity(
                self.titan_inner,
                capacity as usize
            ));
            Ok(())
        }
    }

    pub fn get_blob_cache_capacity(&self) -> u64 {
        unsafe { crocksdb_ffi::ctitandb_options_get_blob_cache_capacity(self.titan_inner) as u64 }
    }

    pub fn set_fifo_compaction_options(&mut self, fifo_opts: FifoCompactionOptions) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_fifo_compaction_options(self.inner, fifo_opts.inner);
        }
    }

    pub fn set_vector_memtable_factory(&mut self, reserved_bytes: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_vector_memtable_factory(self.inner, reserved_bytes);
        }
    }

    pub fn set_doubly_skiplist(&self) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_doubly_skip_list_rep(self.inner);
        }
    }

    pub fn get_memtable_factory_name(&self) -> Option<&str> {
        unsafe {
            let memtable_name =
                crocksdb_ffi::crocksdb_options_get_memtable_factory_name(self.inner);
            if memtable_name.is_null() {
                return None;
            }
            Some(CStr::from_ptr(memtable_name).to_str().unwrap())
        }
    }

    pub fn set_sst_partitioner_factory<F: SstPartitionerFactory>(&mut self, factory: F) {
        let f = new_sst_partitioner_factory(factory);
        unsafe {
            crocksdb_ffi::crocksdb_options_set_sst_partitioner_factory(self.inner, f);
        }
    }

    pub fn set_compact_on_deletion(&self, sliding_window_size: usize, deletion_trigger: usize) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_compact_on_deletion(
                self.inner,
                sliding_window_size,
                deletion_trigger,
            );
        }
    }

    pub fn set_ttl(&mut self, ttl_secs: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_ttl(self.inner, ttl_secs);
        }
    }

    pub fn get_ttl(&self) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_ttl(self.inner) }
    }

    pub fn set_periodic_compaction_seconds(&mut self, secs: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_periodic_compaction_seconds(self.inner, secs);
        }
    }

    pub fn get_periodic_compaction_seconds(&self) -> u64 {
        unsafe { crocksdb_ffi::crocksdb_options_get_periodic_compaction_seconds(self.inner) }
    }

    pub fn set_write_buffer_manager(&mut self, wbm: &WriteBufferManager) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_cf_write_buffer_manager(self.inner, wbm.inner);
        }
    }

    pub fn get_write_buffer_manager(&self) -> Option<WriteBufferManager> {
        let manager =
            unsafe { crocksdb_ffi::crocksdb_options_get_cf_write_buffer_manager(self.inner) };
        if manager.is_null() {
            None
        } else {
            Some(WriteBufferManager { inner: manager })
        }
    }

    pub fn set_bottommost_file_compaction_delay(&mut self, delay: u32) {
        unsafe {
            crocksdb_ffi::crocksdb_options_set_bottommost_file_compaction_delay(self.inner, delay);
        }
    }

    pub fn get_bottommost_file_compaction_delay(&self) -> u32 {
        unsafe { crocksdb_ffi::crocksdb_options_get_bottommost_file_compaction_delay(self.inner) }
    }
}

// ColumnFamilyDescriptor is a pair of column family's name and options.
pub struct ColumnFamilyDescriptor<'a> {
    pub name: &'a str,
    pub options: ColumnFamilyOptions,
}

impl<'a> ColumnFamilyDescriptor<'a> {
    const DEFAULT_COLUMN_FAMILY: &'static str = "default";

    pub fn new(name: &'a str, options: ColumnFamilyOptions) -> Self {
        ColumnFamilyDescriptor { name, options }
    }

    pub fn is_default(&self) -> bool {
        self.name == Self::DEFAULT_COLUMN_FAMILY
    }
}

impl Default for ColumnFamilyDescriptor<'static> {
    fn default() -> Self {
        let name = Self::DEFAULT_COLUMN_FAMILY;
        let options = ColumnFamilyOptions::new();
        ColumnFamilyDescriptor::new(name, options)
    }
}

impl<'a> From<&'a str> for ColumnFamilyDescriptor<'a> {
    fn from(name: &'a str) -> Self {
        let options = ColumnFamilyOptions::new();
        ColumnFamilyDescriptor::new(name, options)
    }
}

impl<'a> From<(&'a str, ColumnFamilyOptions)> for ColumnFamilyDescriptor<'a> {
    fn from(tuple: (&'a str, ColumnFamilyOptions)) -> Self {
        let (name, options) = tuple;
        ColumnFamilyDescriptor::new(name, options)
    }
}

pub struct CColumnFamilyDescriptor {
    inner: *mut crocksdb_ffi::ColumnFamilyDescriptor,
}

impl CColumnFamilyDescriptor {
    pub unsafe fn from_raw(
        inner: *mut crocksdb_ffi::ColumnFamilyDescriptor,
    ) -> CColumnFamilyDescriptor {
        assert!(
            !inner.is_null(),
            "could not new rocksdb column_family_descriptor with null inner"
        );
        CColumnFamilyDescriptor { inner }
    }

    pub fn name(&self) -> &str {
        unsafe {
            let raw_cf_name = crocksdb_ffi::crocksdb_name_from_column_family_descriptor(self.inner);
            CStr::from_ptr(raw_cf_name).to_str().unwrap()
        }
    }

    pub fn options(&self) -> ColumnFamilyOptions {
        unsafe {
            let raw_cf_options =
                crocksdb_ffi::crocksdb_options_from_column_family_descriptor(self.inner);
            ColumnFamilyOptions::from_raw(raw_cf_options, ptr::null_mut())
        }
    }
}

impl Drop for CColumnFamilyDescriptor {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_column_family_descriptor_destroy(self.inner);
        }
    }
}

pub struct FlushOptions {
    pub(crate) inner: *mut DBFlushOptions,
}

impl Default for FlushOptions {
    fn default() -> Self {
        unsafe {
            Self {
                inner: crocksdb_ffi::crocksdb_flushoptions_create(),
            }
        }
    }
}

impl FlushOptions {
    pub fn set_wait(&mut self, wait: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_flushoptions_set_wait(self.inner, wait);
        }
    }

    pub fn set_allow_write_stall(&mut self, allow: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_flushoptions_set_allow_write_stall(self.inner, allow);
        }
    }

    pub fn set_expected_oldest_key_time(&mut self, time: SystemTime) {
        let time = time.duration_since(UNIX_EPOCH).unwrap().as_secs();
        unsafe {
            crocksdb_ffi::crocksdb_flushoptions_set_expected_oldest_key_time(self.inner, time);
        }
    }

    pub fn set_check_if_compaction_disabled(&mut self, check: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_flushoptions_set_check_if_compaction_disabled(self.inner, check);
        }
    }
}

impl Drop for FlushOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_flushoptions_destroy(self.inner);
        }
    }
}

/// IngestExternalFileOptions is used by DB::ingest_external_file
pub struct IngestExternalFileOptions {
    pub(crate) inner: *mut crocksdb_ffi::IngestExternalFileOptions,
}

impl IngestExternalFileOptions {
    pub fn new() -> IngestExternalFileOptions {
        unsafe {
            IngestExternalFileOptions {
                inner: crocksdb_ffi::crocksdb_ingestexternalfileoptions_create(),
            }
        }
    }

    /// If set to false, an ingested file keys could appear in existing snapshots
    /// that where created before the file was ingested.
    pub fn snapshot_consistent(&mut self, whether_consistent: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_set_snapshot_consistency(
                self.inner,
                whether_consistent,
            );
        }
    }

    /// If set to false, DB::ingest_external_file() will fail if the file key range
    /// overlaps with existing keys or tombstones in the DB.
    pub fn allow_global_seqno(&mut self, whether_allow: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_set_allow_global_seqno(
                self.inner,
                whether_allow,
            );
        }
    }

    /// If set to false and the file key range overlaps with the memtable key range
    /// (memtable flush required), DB::ingest_external_file will fail.
    pub fn allow_blocking_flush(&mut self, whether_allow: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_set_allow_blocking_flush(
                self.inner,
                whether_allow,
            );
        }
    }

    /// Set to true to move the files instead of copying them.
    pub fn move_files(&mut self, whether_move: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_set_move_files(
                self.inner,
                whether_move,
            );
        }
    }

    pub fn get_write_global_seqno(&self) -> bool {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_get_write_global_seqno(self.inner)
        }
    }

    /// If set to true, a global_seqno will be written to a given offset in the external SST file
    /// for backward compatibility.
    pub fn set_write_global_seqno(&mut self, whether_write: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_set_write_global_seqno(
                self.inner,
                whether_write,
            );
        }
    }

    pub fn set_verify_checksums_before_ingest(&mut self, whether_verify: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_set_verify_checksums_before_ingest(
                self.inner,
                whether_verify,
            );
        }
    }

    pub fn set_allow_write(&mut self, allow_write: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_set_allow_write(
                self.inner,
                allow_write,
            );
        }
    }
}

impl Drop for IngestExternalFileOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_ingestexternalfileoptions_destroy(self.inner);
        }
    }
}

/// Options while opening a file to read/write
pub struct EnvOptions {
    pub(crate) inner: *mut crocksdb_ffi::EnvOptions,
}

impl EnvOptions {
    pub fn new() -> EnvOptions {
        unsafe {
            EnvOptions {
                inner: crocksdb_ffi::crocksdb_envoptions_create(),
            }
        }
    }
}

impl Drop for EnvOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_envoptions_destroy(self.inner);
        }
    }
}

pub struct RestoreOptions {
    pub(crate) inner: *mut DBRestoreOptions,
}

impl RestoreOptions {
    pub fn new() -> RestoreOptions {
        unsafe {
            RestoreOptions {
                inner: crocksdb_ffi::crocksdb_restore_options_create(),
            }
        }
    }

    pub fn set_keep_log_files(&mut self, flag: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_restore_options_set_keep_log_files(self.inner, flag.into())
        }
    }
}

impl Drop for RestoreOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_restore_options_destroy(self.inner);
        }
    }
}

pub struct FifoCompactionOptions {
    pub(crate) inner: *mut DBFifoCompactionOptions,
}

impl FifoCompactionOptions {
    pub fn new() -> FifoCompactionOptions {
        unsafe {
            FifoCompactionOptions {
                inner: crocksdb_ffi::crocksdb_fifo_compaction_options_create(),
            }
        }
    }

    pub fn set_max_table_files_size(&mut self, max_table_files_size: u64) {
        unsafe {
            crocksdb_ffi::crocksdb_fifo_compaction_options_set_max_table_files_size(
                self.inner,
                max_table_files_size,
            );
        }
    }

    pub fn set_allow_compaction(&mut self, allow_compaction: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_fifo_compaction_options_set_allow_compaction(
                self.inner,
                allow_compaction,
            );
        }
    }
}

impl Drop for FifoCompactionOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_fifo_compaction_options_destroy(self.inner);
        }
    }
}

pub struct LRUCacheOptions {
    pub(crate) inner: *mut DBLRUCacheOptions,
}

impl LRUCacheOptions {
    pub fn new() -> LRUCacheOptions {
        unsafe {
            LRUCacheOptions {
                inner: crocksdb_ffi::crocksdb_lru_cache_options_create(),
            }
        }
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        unsafe {
            crocksdb_ffi::crocksdb_lru_cache_options_set_capacity(self.inner, capacity);
        }
    }

    /// the recommanded shard_bits is 6, also you can set a larger value as long as it is
    /// smaller than 20, also you can set shard_bits to -1, RocksDB will choose a value for you
    /// the recommanded capacity_limit is 0(false) if your memory is sufficient
    /// the recommanded pri_ratio should be 0.05 or 0.1
    pub fn set_num_shard_bits(&mut self, num_shard_bits: c_int) {
        unsafe {
            crocksdb_ffi::crocksdb_lru_cache_options_set_num_shard_bits(self.inner, num_shard_bits);
        }
    }

    pub fn set_strict_capacity_limit(&mut self, strict_capacity_limit: bool) {
        unsafe {
            crocksdb_ffi::crocksdb_lru_cache_options_set_strict_capacity_limit(
                self.inner,
                strict_capacity_limit,
            );
        }
    }

    pub fn set_high_pri_pool_ratio(&mut self, high_pri_pool_ratio: c_double) {
        unsafe {
            crocksdb_ffi::crocksdb_lru_cache_options_set_high_pri_pool_ratio(
                self.inner,
                high_pri_pool_ratio,
            );
        }
    }

    pub fn set_low_pri_pool_ratio(&mut self, low_pri_pool_ratio: c_double) {
        unsafe {
            crocksdb_ffi::crocksdb_lru_cache_options_set_low_pri_pool_ratio(
                self.inner,
                low_pri_pool_ratio,
            );
        }
    }

    pub fn set_memory_allocator(&mut self, allocator: MemoryAllocator) {
        unsafe {
            crocksdb_ffi::crocksdb_lru_cache_options_set_memory_allocator(
                self.inner,
                allocator.inner,
            );
        }
    }
}

impl Drop for LRUCacheOptions {
    fn drop(&mut self) {
        unsafe {
            crocksdb_ffi::crocksdb_lru_cache_options_destroy(self.inner);
        }
    }
}

pub struct HyperClockCacheOptions {
    pub(crate) inner: *mut DBHyperClockCacheOptions,
}

impl HyperClockCacheOptions {
    pub fn new(capacity: usize, estimated_entry_charge: usize) -> HyperClockCacheOptions {
        unsafe {
            HyperClockCacheOptions {
                inner: crocksdb_ffi::crocksdb_hyper_clock_cache_options_create(
                    capacity,
                    estimated_entry_charge,
                ),
            }
        }
    }

    pub fn make_shared_cache(&self) -> Cache {
        unsafe {
            Cache {
                inner: crocksdb_ffi::crocksdb_hyper_clock_cache_options_make_shared_cache(
                    self.inner,
                ),
            }
        }
    }
}

pub struct MergeInstanceOptions {
    pub merge_memtable: bool,
    pub allow_source_write: bool,
    pub max_preload_files: i32,
}

impl Default for MergeInstanceOptions {
    fn default() -> Self {
        Self {
            merge_memtable: false,
            allow_source_write: true,
            max_preload_files: 16,
        }
    }
}
