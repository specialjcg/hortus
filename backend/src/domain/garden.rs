//! Grille du jardin : ensemble de cellules sur une grille carrée.
//!
//! On choisit une grille carrée pour le MVP (lignes de culture naturelles côté potager).
//! Coordonnées (col, row), origine (0,0) = coin Nord-Ouest. Voisins de Von Neumann (4)
//! ou Moore (8) selon le besoin. Chaque cellule représente une surface configurable
//! en m² (par défaut 0.25 m² = 50 cm × 50 cm, taille typique d'une planche serrée).

use serde::{Deserialize, Serialize};

use super::plant::Plant;
use super::soil::{CellState, SoilType};

/// Coordonnées d'une cellule (col, row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellCoord {
    pub col: u16,
    pub row: u16,
}

impl CellCoord {
    pub fn new(col: u16, row: u16) -> Self {
        Self { col, row }
    }
}

/// Cellule = état du sol + éventuelle plante installée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub coord: CellCoord,
    pub state: CellState,
    pub plant: Option<Plant>,
}

impl Cell {
    pub fn new(coord: CellCoord, soil: SoilType) -> Self {
        Self { coord, state: CellState::new(soil), plant: None }
    }
}

/// Grille du jardin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Garden {
    pub cols: u16,
    pub rows: u16,
    /// Surface d'une cellule en m².
    pub cell_area_m2: f64,
    /// Cellules en row-major : index = row * cols + col.
    cells: Vec<Cell>,
}

impl Garden {
    /// Construit un jardin homogène avec un type de sol donné.
    pub fn new_uniform(cols: u16, rows: u16, cell_area_m2: f64, soil: SoilType) -> Self {
        let mut cells = Vec::with_capacity((cols as usize) * (rows as usize));
        for r in 0..rows {
            for c in 0..cols {
                cells.push(Cell::new(CellCoord::new(c, r), soil));
            }
        }
        Self { cols, rows, cell_area_m2, cells }
    }

    /// Surface totale du jardin (m²).
    pub fn total_area_m2(&self) -> f64 {
        (self.cols as f64) * (self.rows as f64) * self.cell_area_m2
    }

    pub fn n_cells(&self) -> usize {
        self.cells.len()
    }

    pub fn in_bounds(&self, c: CellCoord) -> bool {
        c.col < self.cols && c.row < self.rows
    }

    fn idx(&self, c: CellCoord) -> usize {
        (c.row as usize) * (self.cols as usize) + (c.col as usize)
    }

    pub fn get(&self, c: CellCoord) -> Option<&Cell> {
        if self.in_bounds(c) { Some(&self.cells[self.idx(c)]) } else { None }
    }

    pub fn get_mut(&mut self, c: CellCoord) -> Option<&mut Cell> {
        if self.in_bounds(c) {
            let i = self.idx(c);
            Some(&mut self.cells[i])
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Cell> {
        self.cells.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Cell> {
        self.cells.iter_mut()
    }

    /// Voisins de Von Neumann (N/S/E/O), filtrés sur les bords.
    pub fn neighbors4(&self, c: CellCoord) -> Vec<CellCoord> {
        let mut v = Vec::with_capacity(4);
        if c.col > 0 { v.push(CellCoord::new(c.col - 1, c.row)); }
        if c.col + 1 < self.cols { v.push(CellCoord::new(c.col + 1, c.row)); }
        if c.row > 0 { v.push(CellCoord::new(c.col, c.row - 1)); }
        if c.row + 1 < self.rows { v.push(CellCoord::new(c.col, c.row + 1)); }
        v
    }

    /// Voisins de Moore (8 directions).
    pub fn neighbors8(&self, c: CellCoord) -> Vec<CellCoord> {
        let mut v = Vec::with_capacity(8);
        for dr in -1i32..=1 {
            for dc in -1i32..=1 {
                if dr == 0 && dc == 0 { continue; }
                let nc = c.col as i32 + dc;
                let nr = c.row as i32 + dr;
                if nc >= 0 && nr >= 0 && nc < self.cols as i32 && nr < self.rows as i32 {
                    v.push(CellCoord::new(nc as u16, nr as u16));
                }
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_garden_dimensions() {
        let g = Garden::new_uniform(10, 10, 0.25, SoilType::Loam);
        assert_eq!(g.cols, 10);
        assert_eq!(g.rows, 10);
        assert_eq!(g.n_cells(), 100);
        assert_eq!(g.total_area_m2(), 25.0);
    }

    #[test]
    fn cells_are_addressable() {
        let g = Garden::new_uniform(4, 3, 1.0, SoilType::Sand);
        let c = g.get(CellCoord::new(3, 2)).unwrap();
        assert_eq!(c.coord, CellCoord::new(3, 2));
        assert_eq!(c.state.soil_type, SoilType::Sand);
        assert!(g.get(CellCoord::new(4, 0)).is_none());
    }

    #[test]
    fn neighbors4_at_corner() {
        let g = Garden::new_uniform(5, 5, 1.0, SoilType::Loam);
        let n = g.neighbors4(CellCoord::new(0, 0));
        assert_eq!(n.len(), 2);
    }

    #[test]
    fn neighbors4_in_middle() {
        let g = Garden::new_uniform(5, 5, 1.0, SoilType::Loam);
        let n = g.neighbors4(CellCoord::new(2, 2));
        assert_eq!(n.len(), 4);
    }

    #[test]
    fn neighbors8_in_middle() {
        let g = Garden::new_uniform(5, 5, 1.0, SoilType::Loam);
        let n = g.neighbors8(CellCoord::new(2, 2));
        assert_eq!(n.len(), 8);
    }
}
