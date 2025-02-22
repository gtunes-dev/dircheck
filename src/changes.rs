use std::fmt;
use std::str::FromStr;

use crate::database::Database;
use crate::error::DirCheckError;

#[derive(Debug, Default)]
pub struct ChangeCounts {
    add_count: i64,
    modify_count: i64,
    delete_count: i64,
    type_change_count: i64,
    unchanged_count: i64,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ChangeType {
    Add,
    Delete,
    Modify,
    TypeChange,
    NoChange,
}

impl ChangeType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ChangeType::Add => "A",
            ChangeType::Delete => "D",
            ChangeType::Modify => "M",
            ChangeType::TypeChange => "T",
            ChangeType::NoChange => "N",
        }
    }
}

impl ChangeCounts {
    pub fn from_scan_id(db: &Database, scan_id: i64) -> Result<Self, DirCheckError> {
        let conn = &db.conn;
        let mut change_counts = ChangeCounts::default();

        let mut stmt = conn.prepare(
        "SELECT change_type, COUNT(*) FROM changes WHERE scan_id = ? GROUP BY change_type",
        )?;
    
        let mut rows = stmt.query([scan_id])?;
        
        while let Some(row) = rows.next()? {
            let change_type: String = row.get(0)?;
            let count: i64 = row.get(1)?;

            match change_type.as_str() {
                "A" => change_counts.set(ChangeType::Add, count),
                "M" => change_counts.set(ChangeType::Modify, count),
                "D" => change_counts.set(ChangeType::Delete, count),
                "T" => change_counts.set(ChangeType::TypeChange, count),
                _ => println!("Warning: Unknown change type found in DB: {}", change_type),
            }
        }

        Ok(change_counts)
    }
    
    pub fn get(&self, change_type: ChangeType) -> i64 {
        match change_type {
            ChangeType::Add => self.add_count,
            ChangeType::Delete => self.delete_count,
            ChangeType::Modify => self.modify_count,
            ChangeType::TypeChange => self.type_change_count,
            ChangeType::NoChange => self.unchanged_count,
        }
    }

    pub fn increment(&mut self, change_type: ChangeType) {
        let target = match change_type {
            ChangeType::Add => &mut self.add_count,
            ChangeType::Delete => &mut self.delete_count,
            ChangeType::Modify => &mut self.modify_count,
            ChangeType::TypeChange => &mut self.type_change_count,
            ChangeType::NoChange => &mut self.unchanged_count,
       };
       *target += 1;
    }

    pub fn set(&mut self, change_type: ChangeType, count: i64) {
        let target = match change_type {
            ChangeType::Add => &mut self.add_count,
            ChangeType::Delete => &mut self.delete_count,
            ChangeType::Modify => &mut self.modify_count,
            ChangeType::TypeChange => &mut self.type_change_count,
            ChangeType::NoChange => &mut self.unchanged_count,
       };
       *target = count;   
    }
}


impl fmt::Display for ChangeType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let symbol = match self {
            ChangeType::Add => "A",
            ChangeType::Delete => "D",
            ChangeType::Modify => "M",
            ChangeType::TypeChange => "T",
            ChangeType::NoChange => "N",
        };
        write!(f, "{}", symbol)
    }
}

impl FromStr for ChangeType {
    type Err = DirCheckError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "A" => Ok(ChangeType::Add),
            "D" => Ok(ChangeType::Delete),
            "M" => Ok(ChangeType::Modify),
            "T" => Ok(ChangeType::TypeChange),
            "N" => Ok(ChangeType::NoChange),
            _ => Err(DirCheckError::Error(format!("Invalid ChangeType: {}", s))), 
        }
    }
}