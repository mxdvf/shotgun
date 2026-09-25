use libublk::{ctrl::UblkCtrl, BufDesc, UblkError, UblkFlags};
use libublk::io::{BufDescList, UblkDev, UblkIOCtx, UblkQueue};

fn q_fn(qid: u16, dev: &Arc<UblkDev>) {
    let bufs_rc = Rc::new(dev.alloc_queue_io_bufs());
    let bufs = bufs_rc.clone();

    let queue = match UblkQueue::new(qid, dev)
        .unwrap()
        .submit_fetch_commands_unified(BufDescList::Slices(Some(&bufs)))
    {
        Ok(q) => q,
        Err(e) => {
            log::error!("submit_fetch_commands_unified failed: {}", e);
            return;
        }
    };

    queue.wait_and_handle_io(move |q: &UblkQueue, tag: u16, io: &UblkIOCtx| {
        let bufs = bufs_rc.clone();

        // Note: For educational purposes, this shows how slice access can be used
        // even in sync handlers. The slice provides safe bounds-checked access.
        let io_slice = bufs[tag as usize].as_slice();

        // Convert to raw pointer only when required by legacy sync handler API
        // This demonstrates the pattern: use slices for safety, convert to pointers
        // only when absolutely necessary for API compatibility
        __handle_io_cmd(q, tag, io, &io_slice);
    });
}

fn __loop_tgt_calculate_size(f: &std::fs::File) -> Result<(u64, u8, u8)> {
  if let Ok(meta) = f.metadata() {
    if meta.file_type().is_file() {
          // TODO(later): I want to be able to calculate the 9 and 12 figure
          // using the backing file itself. Although I am not really sure,
          // how can a file expose logical and physical sectors, I mean we're
          // anyway emulating the file to be a block device(right?) so it's
          // totally up to us to expose the pbsz and ssz(sector size). I will
          // figure this out later.
          Ok((f.metadata().unwrap().len(), 9, 12))
      } else {
          Err(anyhow::anyhow!("unsupported file"))
      }
  } else {
      Err(anyhow::anyhow!("no file meta got"))
  }
}

// Setting up block device info which could be used later on by
// ublk. But it's not obvious how and why I would want to set this
// up, the code / docs do not specify it clearly(?)
fn init_tgt(dev: &mut UblkDev, loop_tgt: &LoopTgt) -> Result<(), UblkError> {
    log::info!("ublk_shotgun: init_tgt {}", dev.dev_info.dev_id);
    unsafe {
        libc::fcntl(loop_tgt.back_file.as_raw_fd(), libc::F_SETFL, libc::O_DIRECT);
    }

    // Add the backing file to the file descriptors array.
    let tgt = &mut dev.tgt;
    let nr_fds = tgt.nr_fds;
    tgt.fds[nr_fds as usize] = loop_tgt.back_file.as_raw_fd();
    tgt.nr_fds = nr_fds + 1;

    // From what I can understand, this is how we indicate
    // to ublk, the exact config and preference for how to
    // create the underlying ublk device. Because soon after
    // init_tgt is over, is the actual block device created.
    // Nonetheless, it's a bit untuitive (for me at least).
    let sz = { __loop_tgt_calculate_size(&lo.back_file).unwrap() };
    tgt.dev_size = sz.0;
    tgt.params = libublk::sys::ublk_params {
        types: libublk::sys::UBLK_PARAM_TYPE_BASIC,
        basic: libublk::sys::ublk_param_basic {
            logical_bs_shift: sz.1,
            physical_bs_shift: sz.2,
            io_opt_shift: 12,
            io_min_shift: 9,
            max_sectors: dev.dev_info.max_io_buf_bytes >> 9,
            dev_sectors: tgt.dev_size >> 9,
            ..Default::default()
        },
        ..Default::default()
    };
    let val = serde_json::json!({"loop": LoJson { back_file_path: lo.back_file_path.clone(), direct_io: 1 } });
    dev.set_target_json(val);

    Ok(())
}

fn main() {
  // Collect filename from args.
  let args: Vec<String> = env::args().collect();
  if args.len() < 2 {
    panic!("Please supply the backing file!");
  }

  // Setup target and auxiliary stuff.
  let backing_file = String::from(args[1].clone());
  let loop_tgt = LoopTgt {
      back_file: std::fs::OpenOptions::new()
          .read(true)
          .write(true)
          .open(&backing_file)
          .unwrap(),
  };

  // Setup ublk.
  let ctrl = libublk::ctrl::UblkCtrlBuilder::default()
      .name("ublk_shotgun")
      .dev_flags(UblkFlags::UBLK_DEV_F_ADD_DEV)
      .nr_queues(2)
      .build()
      .unwrap();
  let init_tgt_fn = |dev: &mut UblkDev| init_tgt(dev, &loop_tgt);
  let q_handler_fn = |qid: u16, dev: &Arc<UblkDev> | q_handler(qid, dev);
  let post_fn = |ctrl: &UblkCtrl| ctrl.dump();

  // The actual main entrypoint into the ublk framework.
  ctrl.run_target(init_tgt_fn, q_handler_fn, post_fn).unwrap();
}
