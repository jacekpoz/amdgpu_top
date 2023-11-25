use libamdgpu_top::DevicePath;

const APP_NAME: &str = env!("CARGO_PKG_NAME");
const TITLE: &str = env!("TITLE");

mod args;
use args::{AppMode, DumpMode, MainOpt};
mod dump_info;

fn main() {
    let main_opt = MainOpt::parse();
    let (device_path_list, device_path) = {
        let list = DevicePath::get_device_path_list();

        if list.is_empty() {
            eprintln!("There are no the AMD GPU devices found.");
            panic!();
        }

        let device_path = if main_opt.select_apu {
            select_apu(&list)
        } else {
            from_main_opt(&main_opt, &list)
        };

        if main_opt.single_gpu {
            (vec![device_path.clone()], device_path)
        } else {
            (list, device_path)
        }
    };

    #[cfg(feature = "json")]
    if let AppMode::JSON = main_opt.app_mode { match main_opt.dump_mode {
        DumpMode::Info => {
            amdgpu_top_json::dump_json(&device_path_list);
            return;
        },
        DumpMode::Version => {
            amdgpu_top_json::version_json(TITLE);
            return;
        },
        DumpMode::NoDump => {
            let mut j = amdgpu_top_json::JsonApp::new(
                &device_path_list,
                main_opt.refresh_period,
                main_opt.update_process_index,
                main_opt.json_iterations,
                main_opt.no_pc,
            );

            j.run(TITLE);

            return;
        },
        _ => {},
    }}

    match main_opt.dump_mode {
        DumpMode::Info => {
            dump_info::dump_all(TITLE, &device_path_list, main_opt.gpu_metrics);
            return;
        },
        DumpMode::List => {
            device_list(&device_path_list);
            return;
        },
        DumpMode::Process => {
            dump_info::dump_process(TITLE, &device_path_list);
            return;
        },
        DumpMode::Version => {
            println!("{TITLE}");
            return;
        },
        DumpMode::NoDump => if main_opt.gpu_metrics {
            dump_info::dump_gpu_metrics(TITLE, &device_path_list);
            return;
        },
    }

    match main_opt.app_mode {
        AppMode::TUI => {
            #[cfg(feature = "tui")]
            {
                amdgpu_top_tui::run(
                    TITLE,
                    device_path,
                    &device_path_list,
                    main_opt.update_process_index,
                    main_opt.no_pc,
                )
            }
            #[cfg(not(feature = "tui"))]
            {
                eprintln!("\"tui\" feature is not enabled for this build.");
                dump_info::dump(TITLE, &device_path);
            }
        },
        #[cfg(feature = "gui")]
        AppMode::GUI => amdgpu_top_gui::run(
            APP_NAME,
            TITLE,
            &device_path_list,
            device_path.pci,
            main_opt.update_process_index,
            main_opt.no_pc,
        ),
        #[cfg(feature = "json")]
        AppMode::JSON => unreachable!(),
        #[cfg(feature = "tui")]
        AppMode::SMI => amdgpu_top_tui::run_smi(
            TITLE,
            &device_path_list,
            main_opt.update_process_index,
        ),
    }
}

pub fn device_list(list: &[DevicePath]) {
    use libamdgpu_top::AMDGPU::GPU_INFO;

    println!("{TITLE}\n");
    for (i, device_path) in list.iter().enumerate() {
        let Ok(amdgpu_dev) = device_path.init() else { continue };
        let Ok(ext_info) = amdgpu_dev.device_info() else { continue };

        println!("#{i}:");
        println!(
            "    {} ({:#0X}.{:#0X})",
            amdgpu_dev.get_marketing_name_or_default(),
            ext_info.device_id(),
            ext_info.pci_rev_id()
        );
        println!("    {device_path:?}");
    }
}

pub fn from_main_opt(main_opt: &MainOpt, list: &[DevicePath]) -> DevicePath {
    if let Some(pci) = main_opt.pci {
        DevicePath::try_from(pci).unwrap_or_else(|err| {
            eprintln!("{err}");
            eprintln!("pci_path: {pci:?}");
            eprintln!("Device list: {list:#?}");
            panic!();
        })
    } else if let Some(i) = main_opt.instance {
        list
            .get(i)
            .unwrap_or_else(|| {
                eprintln!("index out of bounds: {i}");
                for (i, device) in list.iter().enumerate() {
                    eprintln!("#{i}: {device:#?}");
                }
                panic!();
            })
            .clone()
    } else {
        list.iter().next().unwrap().clone()
    }
}

fn select_apu(list: &[DevicePath]) -> DevicePath {
    use libamdgpu_top::AMDGPU::GPU_INFO;

    list.iter().find(|&device_path| {
        let Ok(amdgpu_dev) = device_path.init() else { return false };
        let Ok(ext_info) = amdgpu_dev.device_info() else { return false };

        ext_info.is_apu()
    }).unwrap_or_else(|| {
        eprintln!("The APU device is not installed or disabled.");
        panic!();
    }).clone()
}
