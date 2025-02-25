use crate::indent::Indent;
use crate::{changes::ChangeType, in_println};
use crate::error::DirCheckError;
use crate::database::Database;
use crate::root_paths::RootPath;
use crate::scans::Scan;
use crate::utils::Utils;

use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use rusqlite::Result;
use tablestream::{Column, Stream};

pub struct Reports {
    // No fields
}

impl Reports {
    const DEFAULT_COUNT: i64 = 10;

    pub fn do_report_scans(db: &Database, scan_id: Option<i64>, count: Option<i64>, changes: bool, items: bool) -> Result<(), DirCheckError> {
        // Handle the single scan case. "Latest" conflicts with "id" so if 
        // the caller specified "latest", scan_id will be None
        if count.is_some() {
            Reports::print_scans(db, count)?;
        } else {
            let scan = Scan::new_from_id(db, scan_id)?;
            let root_path = RootPath::get(db, scan.root_path_id())?;
            let mut stream = Reports::begin_scans_table("Scan");
            stream.row(scan.clone())?;
            Reports::end_scans_table(stream)?;

            if changes {
                Self::print_scan_changes(db, &scan, &root_path)?;
            }

            if items {
                Self::print_scan_items(db, &scan, &root_path)?;
            }
        }

        Ok(())
    }

    pub fn print_scans(db: &Database, count: Option<i64>) -> Result<(), DirCheckError> {

        let mut stream = Reports::begin_scans_table("Scans");
        
        let scan_count = Scan::for_each_scan(
            db, 
            count, 
            |_db, scan| {
                stream.row(scan.clone())?;
                Ok(())
            }
        )?;

        Reports::end_scans_table(stream)?;

        if scan_count == 0 {
            in_println!("No Scans");
        }

        Ok(())
    }

    pub fn report_root_paths(db: &Database, root_path_id: Option<i64>, _path: Option<String>, scans: bool, count: Option<i64>) -> Result<(), DirCheckError> {
        match root_path_id {
            Some(root_path_id) => {
                let root_path = RootPath::get(db, root_path_id)?;
                Self::print_root_path_block(db, &root_path, scans, count)?;
            },
            None => {
                Self::print_root_paths(db, scans, count)?;
            }
        }
        
        Ok(())
    }

    fn print_title(title: &str) {
        in_println!("{}\n{}", title, "=".repeat(title.len()));
    }

    fn print_section_header(header: &str) {
        in_println!("\n{}\n{}", header, "-".repeat(header.len()));
    }

    fn print_none_if_zero(i: i32) {
        if i == 0 {
            in_println!("None.");
        }
    }

    fn begin_scans_table(title: &str) -> Stream<Scan, Stdout> {
        let out = io::stdout();
        let mut stream = Stream::new(out, vec![
            Column::new(|f, s: &Scan| write!(f, "{}", s.id())).header("ID").right().min_width(6),
            Column::new(|f, s: &Scan| write!(f, "{}", s.root_path_id())).header("Path ID").right().min_width(6),
            Column::new(|f, s: &Scan| write!(f, "{}", s.is_deep())).header("Deep").center(),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::format_db_time_short(s.time_of_scan()))).header("Time"),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::opt_i64_or_none_as_str(s.file_count()))).header("Files").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::opt_i64_or_none_as_str(s.folder_count()))).header("Folders").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.is_complete())).header("Complete").center(),

            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().get(ChangeType::Add))).header("Adds").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().get(ChangeType::Modify))).header("Modifies").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().get(ChangeType::Delete))).header("Deletes").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().get(ChangeType::TypeChange))).header("T Changes").right().min_width(7),
        ]);

        stream = stream.title(title);

        stream
    }

    fn end_scans_table(stream: Stream<Scan, Stdout>) -> Result<(), DirCheckError> {
        stream.finish()?;
        Ok(())
    }

    fn print_root_path_block(db: &Database, root_path: &RootPath, scans: bool, count: Option<i64>) -> Result<(), DirCheckError> {
        Self::print_title("Root Path");
        in_println!("Root Path:      {}", root_path.path());
        in_println!("Id:             {}", root_path.id());

        if scans {
            Self::print_root_path_scans(db, root_path.id(), count)?;
        }

        Ok(())
    }

    fn get_tree_path(path_stack: &mut Vec<PathBuf>, root_path: &Path, path: &str, is_dir: bool) -> (usize, PathBuf) {
        // Reduce path to the portion that is relative to the root
        let path = Path::new(path).strip_prefix(root_path).unwrap();
        let parent = path.parent();

        let mut new_path = path;

        // Wind the stack down to the first path that is a parent of the current item
        while let Some(stack_path) = path_stack.last() {
            // if the path at the top of the stack is a prefix of the current path
            // we stop pruning the stack. We now remove the portion of new_path
            // which is covered by the item at the top of the stack - we only
            // want to print the portion that hasn't already been printed
            if path.starts_with(stack_path) {
                new_path = path.strip_prefix(stack_path).unwrap();
                break;
            }
            path_stack.pop();
        }
        if !is_dir {
            if let Some(structural_component) = new_path.parent() {
                let structural_component_str = structural_component.to_string_lossy();
                if !structural_component_str.is_empty() {
                    in_println!("{}{}/", " ".repeat(path_stack.len() * 4), structural_component_str);
                    path_stack.push(parent.unwrap().to_path_buf());

                    // The structural path has been pushed. The new_path is now just the filename
                    new_path = Path::new(new_path.file_name().unwrap());
                }
            }
        }

        let indent_level = path_stack.len();

        // If it's a directory, push it onto the stack
        if is_dir {
            path_stack.push(path.to_path_buf());
        }

        (indent_level, new_path.to_path_buf())
    }
      
    fn print_scan_changes(db: &Database, scan: &Scan, root_path: &RootPath) -> Result<(), DirCheckError> {
        Self::print_section_header("Changed Items");
    
        let root_path = Path::new(root_path.path());
        let mut path_stack: Vec<PathBuf> = Vec::new(); // Stack storing directory paths
    
        // TODO: identify changes as metadata and/or hash
        let change_count = Self::with_each_scan_change(
            db,
            scan.id(),
            |id, change_type, _metadata_changed, _hash_changed, item_type, path| {
                let is_dir = item_type == "D";

                let (indent_level, new_path) = Self::get_tree_path(
                    &mut path_stack, 
                    root_path, 
                    path,
                    is_dir,
                );

                // Print the item
                in_println!("{}[{}] {}{} ({})", 
                    " ".repeat(indent_level * 4), 
                    change_type, 
                    new_path.to_string_lossy(),
                    Utils::dir_sep_or_empty(is_dir),
                    id,
                );
            }
        )?;
               
        Self::print_none_if_zero(change_count);

        Ok(())
    }

    fn with_each_scan_change<F>(db: &Database, scan_id: i64, mut func: F) -> Result<i32, DirCheckError>
    where
        F: FnMut(i64, &str, Option<bool>, Option<bool>, &str, &str),
    {
        let mut change_count = 0;

        let mut stmt = db.conn.prepare(
            "SELECT items.id, changes.change_type, changes.metadata_changed, changes.hash_changed, items.item_type, items.path
            FROM changes
            JOIN items ON items.id = changes.item_id
            WHERE changes.scan_id = ?
            ORDER BY items.path ASC"
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,          // Item ID
                row.get::<_, String>(1)?,       // Change type (A, M, D, etc.)
                row.get::<_, Option<bool>>(2)?, // Metadata Changed
                row.get::<_, Option<bool>>(3)?, // Hash Changed
                row.get::<_, String>(4)?,       // Item type (F, D)
                row.get::<_, String>(5)?,       // Path
            ))
        })?;
        
        for row in rows {
            let (id, change_type, metadata_changed, hash_changed, item_type, path) = row?;

            func(id, &change_type, metadata_changed, hash_changed, &item_type, &path);
            change_count = change_count + 1;
        }
        Ok(change_count)
    }

    fn print_scan_items(db: &Database, scan: &Scan, root_path: &RootPath) -> Result<(), DirCheckError> {
        Self::print_section_header("Items");
        //Self::print_section_header("Items",  "Legend: [Item ID, Item Type, Last Modified, Size] path");

        let root_path = Path::new(root_path.path());
        let mut path_stack: Vec<PathBuf> = Vec::new();

        let item_count = Self::with_each_scan_item(
            db, 
            scan.id(), 
            |id, path, item_type, _last_modified, _file_size, _file_hash| {
                let is_dir = item_type == "D";

                let (indent_level, new_path) = Self::get_tree_path(&mut path_stack, root_path, path, is_dir);

                // Print the item
                in_println!("{}[{}] {}{}",
                    " ".repeat(indent_level * 4), 
                    id,
                    new_path.to_string_lossy(),
                    Utils::dir_sep_or_empty(is_dir),
                );
            }
        )?;

        Self::print_none_if_zero(item_count);
        Ok(())
    }

    pub fn with_each_scan_item<F>(db: &Database, scan_id: i64, mut func: F) -> Result<i32, DirCheckError>
    where
        F: FnMut(i64, &str, &str, i64, Option<i64>, Option<String>),
    {
        let mut item_count = 0;

        let mut stmt = db.conn.prepare(
            "SELECT id, path, item_type, last_modified, file_size, file_hash
            FROM items
            WHERE last_seen_scan_id = ?
            ORDER BY path ASC"
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,              // Item ID
                row.get::<_, String>(1)?,           // Path
                row.get::<_, String>(2)?,           // Item type
                row.get::<_, i64>(3)?,              // Last modified
                row.get::<_, Option<i64>>(4)?,      // File size (can be null
                row.get::<_, Option<String>>(5)?,   // File Hash (can be null)
            ))
        })?;
        
        for row in rows {
            let (id, path, item_type, last_modified, file_size, file_hash) = row?;

            func(id, &path, &item_type, last_modified, file_size, file_hash);
            item_count = item_count + 1;
        }
        Ok(item_count)
    }

    fn print_root_paths(db: &Database, scans: bool, count: Option<i64>) -> Result<(), DirCheckError> {
        RootPath::for_each_root_path(db, scans, count, Self::print_root_path_block)?;

        Ok(())
    }

    fn print_root_path_scans(db: &Database, root_path_id: i64, count: Option<i64>) -> Result<(), DirCheckError> {
        // if count isn't specified, the default is 10
        let count = count.unwrap_or(Self::DEFAULT_COUNT);
        
        if count == 0 {
            return Ok(()); // Nothing to print
        }

        let _in_1 = Indent::new();

        Self::print_section_header("Scans");

        let mut stmt = db.conn.prepare(
            "SELECT id, is_deep, time_of_scan, file_count, folder_count, is_complete
            FROM scans
            WHERE root_path_id = ?
            ORDER BY id DESC
            LIMIT ?"
        )?;

        let rows = stmt.query_map([root_path_id, count], |row| {
            Ok((
                row.get::<_, i64>(0)?,          // scan id
                row.get::<_, bool>(1)?,         // is deep
                row.get::<_, i64>(2)?,          // time of scan
                row.get::<_, Option<i64>>(3)?,  // file count
                row.get::<_, Option<i64>>(4)?,  // folder count
                row.get::<_, bool>(5)?,         // is complete
            ))
        })?;

        let mut printed_header = false;

        for row in rows {
            let (scan_id, is_deep, time_of_scan, file_count, folder_count, is_complete) = row?;
 
            if !printed_header {
                in_println!(
                    "{:<9} {:<6} {:<23} {:<12} {:<14} {:<8}",
                    "ID", "Deep", "Time", "File Count", "Folder Count", "Complete"
                );
                printed_header = true;
            }
            
            in_println!(
                "{:<9} {:<6} {:<23} {:<12} {:<14} {:<8}",
                scan_id,
                is_deep,
                Utils::format_db_time_short(time_of_scan),
                Utils::opt_i64_or_none_as_str(file_count),
                Utils::opt_i64_or_none_as_str(folder_count),
                is_complete,
            );
        }

        if !printed_header {
            in_println!("None")
        }

        Ok(())
    }
     
     /* 

    pub fn do_scans(db: &mut Database, all: bool, count: u64) -> Result<(), DirCheckError> {
        let count: i64 = if all { -1 } else { count as i64 };
        let query = format!("
            SELECT scans.id, scans.scan_time, root_paths.path
            FROM scans
            JOIN root_paths ON scans.root_path_id = root_paths.id
            ORDER BY scans.id DESC
            LIMIT {}",
            count
        );

        let mut stmt = db.conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })?;

        for row in rows {
            let (id, scan_time, path) = row?;

            // Convert scan_time from UNIX timestamp to DateTime<Utc>
            let datetime_utc = DateTime::<Utc>::from_timestamp(scan_time, 0)
                .unwrap_or_default();

            // Convert to local time and format it
            let datetime_local = datetime_utc.with_timezone(&Local);
            let formatted_time = datetime_local.format("%Y-%m-%d %H:%M:%S");

            in_println!("Scan ID: {}, Time: {}, Path: {}", id, formatted_time, path);
        }

        Ok(())
    } */
}