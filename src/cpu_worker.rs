use crate::miner::{Buffer, NonceData};
use crate::reader::ReadReply;
use crossbeam_channel::{Receiver, Sender};
use std::u64;
use tokio::sync::mpsc::Sender as TokioSender;
use std::os::raw::c_char;

extern "C" {
    pub fn find_best_deadline_sph(
        scoops: *mut c_char,
                nonce_count: u64,
        gensig: *mut c_char,
                best_deadline: *mut u64,
                best_offset: *mut u64,
    );
}

pub fn create_cpu_worker_task(
    benchmark: bool,
    thread_pool: rayon::ThreadPool,
    rx_read_replies: Receiver<ReadReply>,
    tx_empty_buffers: Sender<Box<dyn Buffer + Send>>,
    tx_nonce_data: TokioSender<NonceData>,
) -> impl FnOnce() + Send + 'static {
    move || {
        for read_reply in rx_read_replies {
            let task = hash(
                read_reply,
                tx_empty_buffers.clone(),
                tx_nonce_data.clone(),
                benchmark,
            );

            thread_pool.spawn(task);
        }
    }
}

pub fn hash(
    read_reply: ReadReply,
    tx_empty_buffers: Sender<Box<dyn Buffer + Send>>,
    tx_nonce_data: TokioSender<NonceData>,
    benchmark: bool,
) -> impl FnOnce() + Send + 'static {
    move || {
        let mut buffer = read_reply.buffer;
        
        if read_reply.info.len == 0 || benchmark {
            if read_reply.info.finished {
                let deadline = u64::MAX;
                let _ = tx_nonce_data.blocking_send(NonceData {
                        height: read_reply.info.height,
                        block: read_reply.info.block,
                        deadline,
                        nonce: 0,
                        reader_task_processed: read_reply.info.finished,
                        account_id: read_reply.info.account_id,
                });
            }
            let _ = tx_empty_buffers.send(buffer);
            return;
        }

        if read_reply.info.len == 1 && read_reply.info.gpu_signal > 0 {
            return;
        }

        #[allow(unused_assignments)]
        let mut deadline: u64 = u64::MAX;
        #[allow(unused_assignments)]
        let mut offset: u64 = 0;

        let bs = buffer.get_buffer_for_writing();
        let bs = bs.lock().unwrap();

        unsafe {
            find_best_deadline_sph(
                bs.as_ptr() as *mut c_char,
                (read_reply.info.len as u64) / 64,
                read_reply.info.gensig.as_ptr() as *mut c_char,
                &mut deadline,
                &mut offset,
            );
        }

        let _ = tx_nonce_data.blocking_send(NonceData {
                height: read_reply.info.height,
                block: read_reply.info.block,
                deadline,
                nonce: offset + read_reply.info.start_nonce,
                reader_task_processed: read_reply.info.finished,
                account_id: read_reply.info.account_id,
        });

        let _ = tx_empty_buffers.send(buffer);
    }
}

#[cfg(test)]
mod tests {
    use crate::poc_hashing::find_best_deadline_rust;
    use hex;
    use std::os::raw::c_char;
    use std::u64;

        extern "C" {
        pub fn find_best_deadline_sph(
            scoops: *mut c_char,
                    nonce_count: u64,
            gensig: *mut c_char,
                    best_deadline: *mut u64,
                    best_offset: *mut u64,
        );
    }

    #[test]
    fn test_deadline_hashing() {
        let mut deadline: u64;
        let gensig =
            hex::decode("4a6f686e6e7946464d206861742064656e206772f6df74656e2050656e697321")
                .unwrap();

        let mut gensig_array = [0u8; 32];
        gensig_array.copy_from_slice(&gensig[..]);

        let winner: [u8; 64] = [0; 64];
        let loser: [u8; 64] = [5; 64];
        let mut data: [u8; 64 * 32] = [5; 64 * 32];

        for i in 0..32 {
            data[i * 64..i * 64 + 64].clone_from_slice(&winner);

            let result = find_best_deadline_rust(&data, (i + 1) as u64, &gensig_array);
            deadline = result.0;

            assert_eq!(3084580316385335914u64, deadline);
            data[i * 64..i * 64 + 64].clone_from_slice(&loser);
        }
    }

    #[test]
    fn test_c_dispatcher_hashing() {
        let mut deadline: u64 = u64::MAX;
        let mut offset: u64 = 0;
        let gensig =
            hex::decode("4a6f686e6e7946464d206861742064656e206772f6df74656e2050656e697321")
                .unwrap();

        let winner: [u8; 64] = [0; 64];
        let loser: [u8; 64] = [5; 64];
        let mut data: [u8; 64 * 32] = [5; 64 * 32];

        for i in 0..32 {
            data[i * 64..i * 64 + 64].clone_from_slice(&winner);

            unsafe {
                find_best_deadline_sph(
                    data.as_ptr() as *mut c_char,
                        (i + 1) as u64,
                    gensig.as_ptr() as *mut c_char,
                        &mut deadline,
                        &mut offset,
                    );
            }

                    assert_eq!(3084580316385335914u64, deadline);
            assert_eq!(i as u64, offset);
            
                let mut gensig_array = [0; 32];
                gensig_array.copy_from_slice(&gensig[..gensig.len()]);
                let result = find_best_deadline_rust(&data, (i + 1) as u64, &gensig_array);
            assert_eq!(3084580316385335914u64, result.0);
            assert_eq!(i as u64, result.1);

                data[i * 64..i * 64 + 64].clone_from_slice(&loser);
                deadline = u64::MAX;
                offset = 0;
            }
        }
    }