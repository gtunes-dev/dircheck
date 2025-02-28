use crate::changes::ChangeType;
use crate::error::DirCheckError;
use crate::database::Database;
use crate::items::Item;
use crate::root_paths::RootPath;
use crate::scans::Scan;
use crate::utils::Utils;

use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use rusqlite::Result;
use tablestream::*;

pub struct Reports {
    // No fields
}

impl Reports {
    pub fn report_scans(
        db: &Database, 
        scan_id: Option<i64>, 
        latest: bool, 
        count: Option<i64>, 
        changes: bool, 
        items: bool,
    ) -> Result<(), DirCheckError> {
        // Handle the single scan case. "Latest" conflicts with "id" so if 
        // the caller specified "latest", scan_id will be None
        if scan_id.is_none() && !latest {
            Reports::print_scans(db, count)?;
        } else {
            let scan = Scan::new_from_id_else_latest(db, scan_id)?;
            Self::print_scan(db, &scan, changes, items)?;
        }

        Ok(())
    }

    pub fn report_root_paths(db: &Database, root_path_id: Option<i64>, items: bool) -> Result<(), DirCheckError> {
        if root_path_id.is_none() {
            let mut stream = Reports::begin_root_paths_table();
            
            RootPath::for_each_root_path(
                db,
                |rp| {
                    stream.row(rp.clone())?;
                    Ok(())
                }
            )?;

            stream.finish()?;
        } else {
            let root_path_id = root_path_id.unwrap();
            let root_path = RootPath::get(db, root_path_id)?
                .ok_or_else(|| DirCheckError::Error("Root Path Not Found".to_string()))?;
            let mut stream = Self::begin_root_paths_table()
                .title("Root Path");

            stream.row(root_path.clone())?;
            let table_width = stream.finish()?;

            if items {
                let scan_id = root_path.latest_scan(db)?;

                if scan_id.is_none() {
                    Self::print_center(table_width, "No Last Scan - No Items");
                    Self::hr(table_width);
                    return Ok(());
                }

                let scan = Scan::new_from_id_else_latest(db, scan_id)?;

                Self::print_scan(db, &scan, false, true)?;
            }
        }

        Ok(())
    }

    pub fn report_items(db: &Database, item_id: i64) -> Result<(), DirCheckError> {
        let mut stream = Self::begin_items_table("Item", "No Item");

        let item = Item::new(db, item_id)?;
        if item.is_some() {
            stream.row(item.unwrap())?;
        }
        stream.finish()?;

        Ok(())
    }

    fn print_scan(db: &Database, scan: &Scan, changes: bool, items: bool) -> Result<(), DirCheckError> {
        let mut stream = Reports::begin_scans_table("Scan", "No Scan");

        stream.row(scan.clone())?;
        let table_width = stream.finish()?;

        if changes || items {
            let root_path = RootPath::get(db, scan.root_path_id())?
                .ok_or_else(|| DirCheckError::Error("Root Path Not Found".to_string()))?;

            if changes {
                Self::print_scan_changes(db, table_width, &scan, &root_path)?;
            }

            if items {
                Self::print_scan_items(db, table_width, &scan, &root_path)?;
            }
        }

        Ok(())
    }


    fn print_scans(db: &Database, count: Option<i64>) -> Result<(), DirCheckError> {
        let mut stream = Reports::begin_scans_table("Scans", "No Scans");
        
        Scan::for_each_scan(
            db, 
            count, 
            |_db, scan| {
                stream.row(scan.clone())?;
                Ok(())
            }
        )?;

        stream.finish()?;

        Ok(())
    }

    fn begin_scans_table(title: &str, empty_row: &str) -> Stream<Scan, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
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
        ]).title(title).empty_row(empty_row);

        stream
    }

    fn begin_root_paths_table() -> Stream<RootPath, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
            Column::new(|f, rp: &RootPath| write!(f, "{}", rp.id())).header("ID").right().min_width(6),
            Column::new(|f, rp: &RootPath| write!(f, "{}", rp.path())).header("Path").left().min_width(109),
        ]).title("Root Paths").empty_row("No Root Paths");

        stream
    }

    fn begin_items_table(title: &str, empty_row: &str) -> Stream<Item, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
            Column::new(|f, i: &Item| write!(f, "{}", i.id())).header("ID").right().min_width(6),
            Column::new(|f, i: &Item| write!(f, "{}", i.root_path_id())).header("Path ID").right(),
            Column::new(|f, i: &Item| write!(f, "{}", i.last_seen_scan_id())).header("Last Scan").right(),
            Column::new(|f, i: &Item| write!(f, "{}", i.is_tombstone())).header("Tombstone").center(),
            Column::new(|f, i: &Item| write!(f, "{}", i.item_type())).header("Type").center(),
            Column::new(|f, i: &Item| write!(f, "{}", i.path())).header("Path").left(),
            Column::new(|f, i: &Item| write!(f, "{}", Utils::format_db_time_short_or_none(i.last_modified()))).header("Modified").left(),
            Column::new(|f, i: &Item| write!(f, "{}", Utils::opt_i64_or_none_as_str(i.file_size()))).header("Size").right(),
            Column::new(|f, i: &Item| write!(f, "{}", i.file_hash().unwrap_or("-"))).header("Hash").center(),
        ]).title(title).empty_row(empty_row);
        
        stream
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
                    println!("{}{}/", " ".repeat(path_stack.len() * 4), structural_component_str);
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
      
    fn print_scan_changes(db: &Database, width: usize, scan: &Scan, root_path: &RootPath) -> Result<(), DirCheckError> {
        Self::print_center(width, "Changes");
        Self::print_center(width, &format!("Root Path: {}", root_path.path()));

        Self::hr(width);
    
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
                println!("{}[{}] {}{} ({})", 
                    " ".repeat(indent_level * 4), 
                    change_type, 
                    new_path.to_string_lossy(),
                    Utils::dir_sep_or_empty(is_dir),
                    id,
                );
            }
        )?;

        if change_count == 0 {
            Self::print_center(width, "No Changes");
        }

        Self::hr(width);    
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

    fn print_scan_items(db: &Database, width: usize, scan: &Scan, root_path: &RootPath) -> Result<(), DirCheckError> {
        Self::print_center(width, "Items");
        Self::print_center(width, &format!("Root Path: {}", root_path.path()));
        Self::hr(width);

        let root_path = Path::new(root_path.path());
        let mut path_stack: Vec<PathBuf> = Vec::new();

        let item_count = Self::with_each_scan_item(
            db, 
            scan.id(), 
            |id, path, item_type, _last_modified, _file_size, _file_hash| {
                let is_dir = item_type == "D";

                let (indent_level, new_path) = Self::get_tree_path(&mut path_stack, root_path, path, is_dir);

                // Print the item
                println!("{}[{}] {}{}",
                    " ".repeat(indent_level * 4), 
                    id,
                    new_path.to_string_lossy(),
                    Utils::dir_sep_or_empty(is_dir),
                );
            }
        )?;

        if item_count == 0 {
            Self::print_center(width, "No Items");
        }

        Self::hr(width);

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

    fn hr(width: usize) {
        println!("{1:-<0$}", width, ""); 
    }

    fn __print_left(width: usize, value: &str) {
        println!("{0:1$}{3}{0:2$}", "", 0, width - value.len(), value);
    }

    fn print_center(width: usize, value: &str) {
        // determine left padding
        let padding = width - value.len();
        let lpad = padding / 2;
        let rpad = lpad + (padding % 2);
        println!("{0:1$}{3}{0:2$}", "", lpad, rpad, value);

    }
}