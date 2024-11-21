use std::fs::File;
use std::sync::{Arc, Mutex}; 
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::file::reader::{FileReader, SerializedFileReader};
use tracing::{debug, info};
use crate::backtest::data::{Data, DataPtr, POD};
use crate::prelude::Event;

use super::NpyDTyped;
use arrow_array::{UInt64Array, Int64Array, Float64Array};
use rayon::prelude::*;


pub fn read_parquet_file<D: NpyDTyped + Clone>(filepath: &str) -> std::io::Result<Data<D>> {
    let batch_size = 1024 * 1024;
    let events_capacity = 150_000_000;

    let file = File::open(filepath)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)
        .unwrap()
        .with_batch_size(batch_size);
    let reader = builder.build().unwrap();
    let events = Arc::new(Mutex::new(Vec::with_capacity(events_capacity))); 

    // If we use parallel loading here, we need to re-sort by exch_ts in order.
    // This is because exch_ts and local_ts are sorted in chronological order.
    reader.into_iter().par_bridge().for_each(|maybe_batch| { 
        let batch = maybe_batch.unwrap();

        let ev_col = batch.column(0).as_any().downcast_ref::<UInt64Array>().unwrap();
        let exch_ts_col = batch.column(1).as_any().downcast_ref::<Int64Array>().unwrap();
        let local_ts_col = batch.column(2).as_any().downcast_ref::<Int64Array>().unwrap();
        let px_col = batch.column(3).as_any().downcast_ref::<Float64Array>().unwrap();
        let qty_col = batch.column(4).as_any().downcast_ref::<Float64Array>().unwrap();
        let order_id_col = batch.column(5).as_any().downcast_ref::<UInt64Array>().unwrap();
        let ival_col = batch.column(6).as_any().downcast_ref::<Int64Array>().unwrap();
        let fval_col = batch.column(7).as_any().downcast_ref::<Float64Array>().unwrap();

        let mut local_events: Vec<Event> = Vec::with_capacity(batch.num_rows());
        for row in 0..batch.num_rows() {
            local_events.push(Event {
                ev: ev_col.value(row),
                exch_ts: exch_ts_col.value(row),
                local_ts: local_ts_col.value(row),
                px: px_col.value(row),
                qty: qty_col.value(row),
                order_id: order_id_col.value(row),
                ival: ival_col.value(row),
                fval: fval_col.value(row),
            });
        }
        debug!("Read {} events", local_events.len());
        let mut events = events.lock().unwrap(); 
        events.extend(local_events); 
    });

    let mut events = events.lock().unwrap();
    events.par_sort_by_key(|event| event.exch_ts); 
    let data_ptr = DataPtr::new(events.len() * std::mem::size_of::<D>());
    
    // Copy events to DataPtr
    unsafe {
        std::ptr::copy_nonoverlapping(
            events.as_ptr() as *const u8,
            data_ptr.ptr as *mut u8,
            events.len() * std::mem::size_of::<D>()
        );
    }

    let data = unsafe { Data::from_data_ptr(data_ptr, 0) };
    Ok(data)
}
