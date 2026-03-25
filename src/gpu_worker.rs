use crate::miner::{Buffer, NonceData};
use crate::ocl::GpuContext;
use crate::ocl::{gpu_hash, gpu_transfer};
use crate::reader::ReadReply;
use crossbeam_channel::{Receiver, Sender};
use std::sync::Arc;
use std::u64;
use tokio::sync::mpsc;

pub fn create_gpu_worker_task(
    benchmark: bool,
    rx_read_replies: Receiver<ReadReply>,
    tx_empty_buffers: Sender<Box<dyn Buffer + Send>>,
    tx_nonce_data: mpsc::Sender<NonceData>,  // Changed from UnboundedSender to Sender
    context_mu: Arc<GpuContext>,
) -> impl FnOnce() + Send + 'static {
    move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        
        for read_reply in rx_read_replies {
            let buffer = read_reply.buffer;
            if read_reply.info.len == 0 || benchmark {
                if read_reply.info.finished {
                    let deadline = u64::MAX;
                    rt.block_on(async {
                        tx_nonce_data.send(NonceData {
                            height: read_reply.info.height,
                            block: read_reply.info.block,
                            deadline,
                            nonce: 0,
                            reader_task_processed: read_reply.info.finished,
                            account_id: read_reply.info.account_id,
                        }).await.expect("GPU worker failed to send nonce data");
                    });
                }
                let _ = tx_empty_buffers.send(buffer);
                continue;
            }

            if read_reply.info.len == 1 && read_reply.info.gpu_signal > 0 {
                continue;
            }

            gpu_transfer(
                &context_mu,
                buffer.get_gpu_buffers().unwrap(),
                *read_reply.info.gensig,
            );
            let result = gpu_hash(
                &context_mu,
                read_reply.info.len / 64,
                buffer.get_gpu_data().as_ref().unwrap(),
            );
            let deadline = result.0;
            let offset = result.1;

            rt.block_on(async {
                tx_nonce_data.send(NonceData {
                    height: read_reply.info.height,
                    block: read_reply.info.block,

                    deadline,
                    nonce: offset + read_reply.info.start_nonce,
                    reader_task_processed: read_reply.info.finished,
                    account_id: read_reply.info.account_id,
                }).await.expect("GPU worker failed to send nonce data");
            });

            let _ = tx_empty_buffers.send(buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ocl::gpu_hash;
    use crate::ocl::GpuContext;
    use hex;
    use ocl_core as core;
    use std::sync::Arc;
    use std::u64;

    #[test]
    fn test_deadline_hashing() {
        let len: u64 = 16;
        let gensig =
            hex::decode("4a6f686e6e7946464d206861742064656e206772f6df74656e2050656e697321")
                .unwrap();
        let mut data: [u8; 64 * 16] = [0; 64 * 16];
        for i in 0..32 {
            data[i * 32..i * 32 + 32].clone_from_slice(&gensig);
        }

        let context = Arc::new(GpuContext::new(0, 0, 16, false));

        let buffer_gpu = unsafe {
            core::create_buffer::<_, u8>(&context.context, core::MEM_READ_ONLY, 64 * 16, None)
                .unwrap()
        };

        unsafe {
            core::enqueue_write_buffer(
                &context.queue_transfer,
                &context.gensig_gpu,
                true,
                0,
                &gensig,
                None,
                None,
            )
            .unwrap();
        }

        unsafe {
            core::enqueue_write_buffer(
                &context.queue_transfer,
                &buffer_gpu,
                true,
                0,
                &data,
                None,
                None,
            )
            .unwrap();
        }

        let result = gpu_hash(&context, len as usize, &buffer_gpu);
        assert_eq!(18043101931632730606u64, result.0);
    }
}