//! One approved set of immutable DAT archives for a server process.
use crate::{DatArchive, DatError};

#[derive(Clone)]
pub struct DatStore {
    portal: DatArchive,
    cell: DatArchive,
    language: DatArchive,
}

impl DatStore {
    pub fn new(
        portal: DatArchive,
        cell: DatArchive,
        language: DatArchive,
    ) -> Result<Self, DatError> {
        if !portal.is_approved() || !cell.is_approved() || !language.is_approved() {
            return Err(DatError::Format("DAT store requires approved archives"));
        }
        if portal.header().dataset != 1
            || cell.header().dataset != 2
            || language.header().dataset != 3
        {
            return Err(DatError::Format("incorrect DAT store datasets"));
        }
        Ok(Self {
            portal,
            cell,
            language,
        })
    }

    pub fn portal(&self) -> DatArchive {
        self.portal.clone()
    }

    pub fn cell(&self) -> DatArchive {
        self.cell.clone()
    }

    pub fn language(&self) -> DatArchive {
        self.language.clone()
    }
}
