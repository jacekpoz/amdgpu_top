use std::fmt::Write;
use cursive::align::HAlign;
use cursive::view::{Nameable, Scrollable};
use cursive::views::{HideableView, LinearLayout, TextContent, TextView, Panel};

use libamdgpu_top::AMDGPU::MetricsInfo;
use libamdgpu_top::{stat, DevicePath, Sampling};
use stat::{GfxoffMonitor, GfxoffStatus, FdInfoSortType};

use crate::{Text, AppTextView};

const GPU_NAME_LEN: usize = 25;
const LINE_LEN: usize = 150;
const THR_LEN: usize = 48;
const ECC_LABEL: &str = "ECC_UnCorr.";
const ECC_LEN: usize = ECC_LABEL.len()-2;
const PROC_TITLE: &str = "Processes";

use libamdgpu_top::app::AppAmdgpuTop;
use std::collections::HashMap;

struct SmiApp {
    app_amdgpu_top: AppAmdgpuTop,
    index: usize,
    gfxoff_monitor: Option<GfxoffMonitor>,
    fdinfo_view: AppTextView,
    info_text: Text,
}

impl SmiApp {
    pub fn new(app_amdgpu_top: AppAmdgpuTop, index: usize) -> Option<Self> {
        let gfxoff_monitor = GfxoffMonitor::new(app_amdgpu_top.device_path.pci).ok();

        Some(Self {
            app_amdgpu_top,
            index,
            gfxoff_monitor,
            fdinfo_view: Default::default(),
            info_text: Default::default(),
        })
    }

    fn info_header() -> TextView {
        let text = format!(concat!(
            "GPU {name:<name_len$} {pad:10}|{pci:<16}|{vram:^18}|\n",
            "SCLK    MCLK    VDDGFX  Power           | GFX% UMC%Media%|{gtt:^18}|\n",
            "Temp    {fan:<7} {ecc} {thr:<THR_LEN$}|"
            ),
            name = "Name",
            name_len = GPU_NAME_LEN,
            pci = " PCI Bus",
            vram = "VRAM Usage",
            gtt = " GTT Usage",
            pad = "",
            fan = "Fan",
            ecc = "ECC_UnCorr.",
            thr = "Throttle_Status",
            THR_LEN = THR_LEN,
        );

        TextView::new(text).no_wrap()
    }

    fn info_text(&mut self) -> TextView {
        TextView::new_with_content(self.info_text.content.clone()).no_wrap()
    }

    fn fdinfo_panel(&self) -> Panel<TextView> {
        let text = TextView::new_with_content(self.fdinfo_view.text.content.clone()).no_wrap();
        Panel::new(text)
            .title(format!(
                "#{:<2} {}",
                self.index,
                self.app_amdgpu_top.device_info.marketing_name,
            ))
            .title_position(HAlign::Left)
    }

    fn update_info_text(&mut self) -> Result<(), std::fmt::Error> {
        let sensors = self.app_amdgpu_top.stat.sensors.as_ref();
        self.info_text.clear();

        writeln!(
            self.info_text.buf,
            "#{i:<2} [{name:GPU_NAME_LEN$}]({gfx_ver:>7})| {pci}   |{vu:6}/{vt:6} MiB |",
            i = self.index,
            name = self.app_amdgpu_top.device_info.marketing_name
                .get(..GPU_NAME_LEN)
                .unwrap_or_else(|| &self.app_amdgpu_top.device_info.marketing_name),
            gfx_ver = match &self.app_amdgpu_top.device_info.gfx_target_version {
                Some(ver) => &ver,
                None => "",
            },
            pci = self.app_amdgpu_top.device_info.pci_bus,
            vu = self.app_amdgpu_top.stat.vram_usage.0.vram.heap_usage >> 20,
            vt = self.app_amdgpu_top.stat.vram_usage.0.vram.total_heap_size >> 20,
        )?;

        if let Some(sclk) = sensors.and_then(|s| s.sclk) {
            write!(self.info_text.buf, "{sclk:4}MHz ")?;
        } else {
            write!(self.info_text.buf, "____MHz ")?;
        }
        if let Some(mclk) = sensors.and_then(|s| s.mclk) {
            write!(self.info_text.buf, "{mclk:4}MHz ")?;
        } else {
            write!(self.info_text.buf, "____MHz ")?;
        }

        if let Some(vddgfx) = sensors.and_then(|s| s.vddgfx) {
            write!(self.info_text.buf, "{vddgfx:4}mV ")?;
        } else {
            write!(self.info_text.buf, "____mV ")?;
        }

        match (
            sensors.and_then(|s| s.any_hwmon_power()),
            sensors.and_then(|s| s.power_cap.as_ref()),
        ) {
            (Some(power), Some(cap)) =>
                write!(self.info_text.buf, " {:>3}/{:>3}W ", power.value, cap.current)?,
            (Some(power), None) => write!(self.info_text.buf, " {:>3}/___W ", power.value)?,
            _ => write!(self.info_text.buf, " ___/___W ")?,
        }

        if let Some(ref mut gfxoff_monitor) = self.gfxoff_monitor {
            let _ = gfxoff_monitor.update();

            match gfxoff_monitor.status {
                GfxoffStatus::InGFXOFF => write!(self.info_text.buf, "GFXOFF |")?,
                _ => write!(self.info_text.buf, "       |")?,
            }
        } else {
            write!(self.info_text.buf, "       |")?;
        }

        for usage in [
            self.app_amdgpu_top.stat.activity.gfx,
            self.app_amdgpu_top.stat.activity.umc,
            self.app_amdgpu_top.stat.activity.media,
        ] {
            if let Some(usage) = usage {
                write!(self.info_text.buf, " {usage:>3}%")?;
            } else {
                write!(self.info_text.buf, " ___%")?;
            }
        }

        writeln!(
            self.info_text.buf,
            " |{gu:>6}/{gt:>6} MiB |",
            gu = self.app_amdgpu_top.stat.vram_usage.0.gtt.heap_usage >> 20,
            gt = self.app_amdgpu_top.stat.vram_usage.0.gtt.total_heap_size >> 20,
        )?;

        if let Some(temp) = sensors.and_then(|s| s.edge_temp.as_ref()) {
            write!(self.info_text.buf, " {:>3}C ", temp.current)?;
        } else {
            write!(self.info_text.buf, " ___C ")?;
        }

        if let Some(fan_rpm) = sensors.and_then(|s| s.fan_rpm) {
            write!(self.info_text.buf, "  {fan_rpm:4}RPM ")?;
        } else {
            write!(self.info_text.buf, "  ____RPM ")?;
        }

        if let Some(ecc) = &self.app_amdgpu_top.stat.memory_error_count {
            write!(self.info_text.buf, "[{:>ECC_LEN$}] ", ecc.uncorrected)?;
        } else {
            write!(self.info_text.buf, "[{:>ECC_LEN$}] ", "N/A")?;
        }

        if let Some(thr) = self.app_amdgpu_top.stat.metrics.as_ref().and_then(|m| m.get_throttle_status_info()) {
            let thr = format!("{:?}", thr.get_all_throttler());
            write!(
                self.info_text.buf,
                "{:<THR_LEN$}|",
                thr.get(..THR_LEN).unwrap_or_else(|| &thr)
            )?;
        } else {
            write!(
                self.info_text.buf,
                "{:<THR_LEN$}|",
                "N/A",
            )?;
        }

        self.info_text.set();

        Ok(())
    }

    fn update(&mut self, sample: &Sampling) {
        self.app_amdgpu_top.update(sample.to_duration());

        let _ = self.fdinfo_view.print_fdinfo(
            &mut self.app_amdgpu_top.stat.fdinfo,
            FdInfoSortType::default(),
            false,
        );

        let _ = self.update_info_text();
        self.fdinfo_view.text.set();
    }
}

struct SuspendedApp {
    device_path: DevicePath,
    index: usize,
    fdinfo_view: AppTextView,
    info_text: Text,
}

impl SuspendedApp {
    fn new(device_path: DevicePath, index: usize) -> Self {
        let mut info_text: Text = Default::default();

        let _ = writeln!(
            info_text.buf,
            "#{index:<2} [{name:<20} ({did:#X}:{rid:#X})]| {pci}   | Suspended",
            name = device_path.device_name,
            did = device_path.device_id,
            rid = device_path.revision_id,
            pci = device_path.pci,
        );

        info_text.set();

        Self {
            device_path,
            index,
            fdinfo_view: Default::default(),
            info_text,
        }
    }

    fn info_text(&mut self) -> TextView {
        TextView::new_with_content(self.info_text.content.clone()).no_wrap()
    }

    fn fdinfo_panel(&self) -> Panel<TextView> {
        let text = TextView::new_with_content(self.fdinfo_view.text.content.clone()).no_wrap();
        Panel::new(text)
            .title(format!(
                "#{:<2} {}",
                self.index,
                self.device_path.device_name,
            ))
            .title_position(HAlign::Left)
    }

    fn to_smi_app(&self) -> Option<SmiApp> {
        let amdgpu_dev = self.device_path.init().ok()?;
        let app_amdgpu_top = AppAmdgpuTop::new(amdgpu_dev, self.device_path.clone(), &Default::default())?;
        let gfxoff_monitor = GfxoffMonitor::new(self.device_path.pci).ok();

        Some(SmiApp {
            app_amdgpu_top,
            index: self.index,
            gfxoff_monitor,
            fdinfo_view: self.fdinfo_view.clone(),
            info_text: self.info_text.clone(),
        })
    }
}

pub fn run_smi(title: &str, device_path_list: &[DevicePath], interval: u64) {
    let sample = Sampling::low();
    let (vec_app, suspended) = AppAmdgpuTop::create_app_and_suspended_list(
        device_path_list,
        &Default::default(),
    );
    let mut vec_app: Vec<_> = vec_app
        .into_iter()
        .enumerate()
        .filter_map(|(i, app)| SmiApp::new(app, i))
        .collect();
    let app_len = vec_app.len();
    let mut sus_app_map: HashMap<_, _> = suspended
        .into_iter()
        .enumerate()
        .map(|(i, (pci, device_path))| (
            pci,
            SuspendedApp::new(device_path.clone(), app_len+i),
        ))
        .collect();

    let mut siv = cursive::default();
    {
        let mut layout = LinearLayout::vertical().child(TextView::new(title));
        let line = TextContent::new(format!("{:->LINE_LEN$}", ""));
        {
            let mut info = LinearLayout::vertical()
                .child(SmiApp::info_header())
                .child(TextView::new_with_content(line.clone()).no_wrap());
            for app in vec_app.iter_mut() {
                app.update(&sample);
                info.add_child(app.info_text());
                info.add_child(TextView::new_with_content(line.clone()).no_wrap());
            }
            for (_pci, sus_app) in sus_app_map.iter_mut() {
                info.add_child(sus_app.info_text());
                info.add_child(TextView::new_with_content(line.clone()).no_wrap());
            }
            info.remove_child(info.len()-1);
            layout.add_child(Panel::new(info));
        }
        {
            let mut proc = LinearLayout::vertical();
            for app in &vec_app {
                proc.add_child(app.fdinfo_panel());
            }
            for (_pci, sus_app) in sus_app_map.iter_mut() {
                proc.add_child(sus_app.fdinfo_panel());
            }
            let h = HideableView::new(proc).with_name(PROC_TITLE);
            layout.add_child(Panel::new(h).title(PROC_TITLE).title_position(HAlign::Left));
        }
        layout.add_child(TextView::new("\n(p)rocesses (q)uit"));

        siv.add_fullscreen_layer(
            layout
                .scrollable()
                .scroll_y(true)
        );
    }

    {
        let device_paths: Vec<DevicePath> = device_path_list.to_vec();
        stat::spawn_update_index_thread(device_paths, interval);
    }

    siv.add_global_callback('q', cursive::Cursive::quit);
    siv.add_global_callback('p', |s| {
        s.call_on_name(PROC_TITLE, |view: &mut HideableView<LinearLayout>| {
            view.set_visible(!view.is_visible());
        });
    });
    siv.set_theme(cursive::theme::Theme::terminal_default());

    let cb_sink = siv.cb_sink().clone();
    let mut remove_sus_devices = Vec::new();

    std::thread::spawn(move || loop {
        std::thread::sleep(sample.to_duration()); // 1s

        for pci in &remove_sus_devices {
            let _ = sus_app_map.remove(pci);
        }

        if !remove_sus_devices.is_empty() {
            remove_sus_devices.clear();
            remove_sus_devices.shrink_to_fit();
        }

        for app in vec_app.iter_mut() {
            app.update(&sample);
        }

        for (pci, sus_app) in sus_app_map.iter() {
            if sus_app.device_path.check_if_device_is_active() {
                let Some(smi_app) = sus_app.to_smi_app() else { continue };
                vec_app.push(smi_app);
                remove_sus_devices.push(*pci);
            }
        }

        cb_sink.send(Box::new(cursive::Cursive::noop)).unwrap();
    });

    siv.run();
}
