extern crate hyper;

use hyper::client::Client;
use hyper::header::ContentLength;

use std::env;
use std::fmt;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File, OpenOptions};
use std::io::prelude::*;
use std::io;
use std::path::{Path, PathBuf};

const KF2_CDN: &'static str = "http://kf2.tripwirecdn.com/";

fn main() {
    let mut args = env::args().peekable();

    // Eat the executable name
    let _ = args.next(); 

    if args.peek().is_none() {
        println!("Map name(s) not given! Expected one or more map names, separated by spaces.");
        println!("E.g.: kf2_map_install KF-Biolab KF-Outpost KF-BurningParis");
        return;
    }

    let kf2_install_folder: PathBuf = env::var("KF2_INSTALL").ok()
        .expect("%KF2_INSTALL% not set!").into();

    let _ = fs::metadata(&kf2_install_folder).ok()
        .expect("%KF2_INSTALL% folder does not exist!");

    let kf2_config_file = kf2_install_folder.join("KFGame/Config/PCServer-KFGame.ini");

    let kf2_map_folder = kf2_install_folder.join("KFGame/BrewedPC/Maps/");    

    let mut kf2_config = OpenOptions::new().write(true).append(true)
        .open(&kf2_config_file)
        .expect("Could not open configuration file PCServer-KFGame.ini");

    let mut client = Client::new();

    for map_name in args {
        download_map_file(&mut client, &map_name, &kf2_map_folder);

        println!("File download completed. Adding map to config...");

        update_config(&mut kf2_config, &map_name);

        println!("Server config updated.");
    }

    println!("Operation(s) completed. Exiting.");
}

fn download_map_file(client: &mut Client, map_name: &str, map_folder: &Path) {
    let map_filename = format!("{}.kfm", map_name);
    let map_path = map_folder.join(&map_filename);

    let _ = fs::metadata(&map_path)
        .err().expect("Map file already exists!");

    let download_url = format!("{}{}", KF2_CDN, map_filename);
    
    println!("Fetching {}", download_url);

    let mut res = client.get(&download_url).send()
        .expect("Error connecting to Tripwire CDN: ");

    assert!(res.status.is_success(), "Failed to fetch URL. Exiting.");

    print!("Success! File size: ");

    let maybe_file_size = res.headers.get::<ContentLength>()
        .map(|&ContentLength(len)| len);

    let mut download_file = DownloadFile {
        path: &map_path,
        completed: false,
    };

    if let Some(file_size) = maybe_file_size {
        let size_kb = file_size as f64 / 1000.0;
        println!("{:.2} KB", size_kb);

        let mut map_file = File::create(&map_path)
            .expect("Error opening map file for writing: ");

        let mut progress_bar = ProgressBar {
            current: 0,
            total: file_size,
        };

        let cb = |so_far| {
            let so_far_kb = so_far as f64 / 1000.0;
            progress_bar.current = so_far;

            print!("\rDownloaded: {:.2} KB / {:.2} KB {}", so_far_kb, size_kb, progress_bar);
        };

        copy_with_cb(&mut res, &mut map_file, cb).unwrap();
    } else {
        println!("unknown.");

        let mut map_file = File::create(&map_path)
            .expect("Error opening map file for writing: ");

        copy_with_cb(&mut res, &mut map_file, |so_far| {
            let so_far_kb = so_far as f64 / 1000.0;

            print!("\rDownloaded: {:.2} KB", so_far_kb);
        }).unwrap();
    }

    download_file.completed = true;

    // Clear line and return cursor to start
    print!("\r\x1b[K");
}

fn update_config(kf2_config: &mut File, map_name: &str) {
    write!(
        kf2_config, 
        "
[{0} KFMapSummary]
MapName={0}
ScreenshotPathName=UI_MapPreview_TEX.UI_MapPreview_Placeholder
        ",
        map_name
    ).unwrap();
}

fn copy_with_cb<R: Read, W: Write, F: FnMut(u64)>(rdr: &mut R, wrt: &mut W, mut cb: F) -> io::Result<u64>{
    let mut buf = [0; 64 * 1024];
    let mut written = 0;

    loop {
        let len = match rdr.read(&mut buf) {
            Ok(0) => return Ok(written),
            Ok(len) => len,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        try!(wrt.write_all(&buf[..len]));
        written += len as u64;

        cb(written);
    }
}

struct ProgressBar {
    current: u64,
    total: u64,
}

impl fmt::Display for ProgressBar {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let pct_i = (self.current * 200 + self.total) / (self.total * 2);
        let bars_filled = pct_i / 5;
        let bars_empty = 20 - bars_filled;

        fmt.write_char('[').and_then(|_| {
            for _ in 0 .. bars_filled {
                try!(fmt.write_char('='));
            }

            Ok(())
        }).and_then(|_| {
            for _ in 0 .. bars_empty {
                try!(fmt.write_char('-'));
            }

            Ok(())
        })
        .and_then(|_| fmt.write_char(']'))
        .and_then(|_| fmt.write_fmt(format_args!("{}%", pct_i)))
    }
}

struct DownloadFile<'a> {
    path: &'a Path,
    completed: bool,
}

impl<'a> Drop for DownloadFile<'a> {
    fn drop(&mut self) {
        if !self.completed && fs::metadata(self.path).is_ok() {
            println!("\nDeleting incomplete file...");
            fs::remove_file(self.path);
        }
    }
}
