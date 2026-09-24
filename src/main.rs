use libublk::{ctrl::UblkCtrl, BufDesc, UblkError, UblkFlags};
use libublk::io::{BufDescList, UblkDev, UblkIOCtx, UblkQueue};

fn main() {
  // Setup target and auxiliary stuff.
  let backing_file = String::from("./illusion.img")
  let loop_tgt = LoopTgt {
      back_file: std::fs::OpenOptions::new()
          .read(true)
          .write(true)
          .open(&backing_file)
          .unwrap(),
      direct_io: 1,
  };

  // Setup ublk.
  let ctrl = libublk::ctrl::UblkCtrlBuilder::default()
      .name("ublk_shotgun")
      .id(id)
      .dev_flags(UblkFlags::UBLK_DEV_F_ADD_DEV)
      .nr_queues(2)
      .build()
      .unwrap();
  let tgt_fn = |dev: &mut UblkDev| loop_init_tgt(dev, &loop_tgt);
  let q_handler = move |qid, dev: &_| q_fn(qid, dev);
  let post_start_action = move |ctrl: &UblkCtrl| ctrl.dump();

  // The actual main entrypoint into the ublk framework.
  ctrl.run_target(tgt_fn, q_handler, post_start_action).unwrap();
}
