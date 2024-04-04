use std::io::{self, Read, Write};

use libc;
use rand::Rng;
use termios::{tcsetattr, Termios, ECHO, ICANON, TCSANOW};

macro_rules! clear_term {
    () => {
        // Clear screen and render field at the top
        print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
    };
}

macro_rules! print_flush {
    ($($t:tt)*) => {
        {
            write!(std::io::stdout(), $($t)*).unwrap();
            std::io::stdout().flush().unwrap();
        }
    }
}

macro_rules! println_flush {
    () => {
        println!();
        std::io::stdout().flush().unwrap();
    };
    ($($t:tt)*) => {
        {
            write!(std::io::stdout(), $($t)*).unwrap();
            println!();
            std::io::stdout().flush().unwrap();
        }
    }
}

const STDIN_FILENO: libc::c_int = 0;

const PIPEBOMB: &str = "@";
const FLAGGED: &str = ">";
const CLOSED: &str = ".";

#[derive(Clone, PartialEq)]
enum State {
    Open,
    Closed,
    Flagged,
}

enum Orientation {
    Vertical,
    Horizontal,
}

#[derive(Clone)]
struct Cell {
    state: State,
    pipebomb: bool,
}

impl Cell {
    fn empty() -> Self {
        Cell {
            state: State::Closed,
            pipebomb: false,
        }
    }
}

struct Field {
    rows: usize,
    cols: usize,
    cells: Vec<Vec<Cell>>,
    bomb_pcnt: usize,
    cursor: [usize; 2],
}

impl Field {
    fn new(rows: usize, cols: usize, bomb_pcnt: usize) -> Self {
        let mut cells = Vec::new();
        for i in 0..rows {
            cells.push(vec![Cell::empty(); cols]);
        }
        let bomb_pcnt = if bomb_pcnt > 100 { 100 } else { bomb_pcnt };

        Self {
            rows,
            cols,
            cells,
            bomb_pcnt,
            cursor: [0, 0],
        }
    }

    fn has_bomb_at(&self, row: usize, col: usize) -> bool {
        self.cells[row][col].pipebomb
    }

    fn is_cursor_at(&self, row: usize, col: usize) -> bool {
        row == self.cursor[0] && col == self.cursor[1]
    }

    fn set_bomb_at(&mut self, row: usize, col: usize) -> bool {
        let has_bomb = self.has_bomb_at(row, col);
        if !has_bomb && !self.is_cursor_at(row, col) {
            self.cells[row][col].pipebomb = true;
            return true;
        }
        return false;
    }

    /// Resets the field & randomizes it:
    fn randomize(&mut self) {
        // Reset all cells:
        for i in 0..self.rows {
            for j in 0..self.cols {
                self.cells[i][j] = Cell::empty();
            }
        }

        let bomb_count = (self.rows * self.rows * self.bomb_pcnt + 99) / 100;
        let mut rng = rand::thread_rng();
        for i in 0..bomb_count {
            let row = rng.gen_range(0..self.rows);
            let col = rng.gen_range(0..self.cols);

            // Loop to avoid placing bombs on spots that already contain one:
            while self.set_bomb_at(row, col) {}
        }
    }

    //
    fn cell_str_at(&self, row: usize, col: usize) -> String {
        if self.has_bomb_at(row, col) {
            return PIPEBOMB.to_owned();
        }

        let mut bomb_count = 0u8;
        for i in -1..=1 {
            for j in -1..=1 {
                if i == 0 && j == 0 {
                    continue;
                }
                let r = row as isize + i;
                let c = col as isize + j;

                if r < 0 || r >= self.rows as isize || c < 0 || c >= self.cols as isize {
                    continue;
                }

                // If not out of bounds, use them to index sorrounding cells:
                if self.has_bomb_at(r as usize, c as usize) {
                    bomb_count += 1;
                }
            }
        }
        return if bomb_count > 0 {
            bomb_count.to_string()
        } else {
            " ".to_owned()
        };
    }

    fn get_cell_mut(&mut self, row: usize, col: usize) -> &mut Cell {
        &mut self.cells[row][col]
    }

    fn out_of_bounds(&self, irow: isize, icol: isize) -> (bool, bool) {
        (
            irow < 0 || irow >= self.rows as isize,
            icol < 0 || icol >= self.cols as isize,
        )
    }

    fn bombs_around(&self, irow: isize, icol: isize) -> u32 {
        let mut bomb_count = 0u32;
        for i in -1..=1 {
            for j in -1..=1 {
                if i == 0 && j == 0 {
                    continue;
                }
                let r = irow as isize + i;
                let c = icol as isize + j;

                if r < 0 || r >= self.rows as isize || c < 0 || c >= self.cols as isize {
                    continue;
                }

                // If not out of bounds, use them to index sorrounding cells:
                if self.has_bomb_at(r as usize, c as usize) {
                    bomb_count += 1;
                }
            }
        }
        return bomb_count;
    }

    fn open_at(&mut self, row: usize, col: usize) {
        self.cells[row][col].state = State::Open
    }

    fn check_at(&mut self, row: usize, col: usize) {
        if self.cells[row][col].pipebomb {
            return;
        }
        if self.bombs_around(row as isize, col as isize) > 0 {
            self.open_at(row, col);
            return;
        }

        match self.cells[row][col].state {
            State::Open => return,
            State::Closed => self.open_at(row, col),
            _ => (),
        }

        let positive_oob = self.out_of_bounds(row as isize + 1, col as isize + 1);
        let negative_oob = self.out_of_bounds(row as isize - 1, col as isize - 1);

        // Up
        if !negative_oob.0 {
            self.check_at(row - 1, col);
        }

        // Left
        if !negative_oob.1 {
            self.check_at(row, col - 1);
        }

        // Down
        if !positive_oob.0 {
            self.check_at(row + 1, col);
        }

        // Right
        if !positive_oob.1 {
            self.check_at(row, col + 1);
        }

        // Diag UL
        if !negative_oob.0 && !negative_oob.1 {
            self.check_at(row - 1, col - 1);
        }

        // Diag DL
        if !positive_oob.0 && !negative_oob.1 {
            self.check_at(row + 1, col - 1);
        }

        // Diag UR
        if !negative_oob.0 && !positive_oob.1 {
            self.check_at(row - 1, col + 1);
        }

        // Diag DR
        if !positive_oob.0 && !positive_oob.1 {
            self.check_at(row + 1, col + 1);
        }
    }

    // TODO: Open recursively empty spaces
    fn open_at_cursor(&mut self, buffer: &mut [u8]) -> bool {
        let row = self.cursor[0];
        let col = self.cursor[1];
        match self.cells[row][col].state {
            State::Closed => self.check_at(row, col),
            State::Flagged => {
                print_flush!("\nAre you sure you want to open this flagged cell? (Y/N): ");
                loop {
                    std::io::stdin().read_exact(buffer).unwrap();
                    match buffer[0] as char {
                        'Y' | 'y' => {
                            // cell.state = State::Open;
                            self.check_at(row, col);
                            break;
                        }
                        'N' | 'n' => {
                            break;
                        }
                        _ => (),
                    }
                }
            }
            _ => (),
        }
        self.cells[row][col].pipebomb
    }

    fn flag_at_cursor(&mut self) {
        let mut cell: &mut Cell = self.get_cell_mut(self.cursor[0], self.cursor[1]);
        match cell.state {
            State::Closed => cell.state = State::Flagged,
            State::Flagged => cell.state = State::Closed,
            _ => (),
        }
    }

    fn dec_cursor(&mut self, o: Orientation) {
        match o {
            Orientation::Vertical => {
                if self.cursor[0] > 0 {
                    self.cursor[0] -= 1;
                }
            }
            Orientation::Horizontal => {
                if self.cursor[1] > 0 {
                    self.cursor[1] -= 1;
                }
            }
        }
    }

    fn inc_cursor(&mut self, o: Orientation) {
        match o {
            Orientation::Vertical => {
                if self.cursor[0] < self.rows - 1 {
                    self.cursor[0] += 1;
                }
            }
            Orientation::Horizontal => {
                if self.cursor[1] < self.cols - 1 {
                    self.cursor[1] += 1;
                }
            }
        }
    }

    fn reveal_mines(&mut self) {
        for i in 0..self.rows {
            for j in 0..self.cols {
                let mut cell = self.get_cell_mut(i, j);
                if cell.pipebomb {
                    cell.state = State::Open;
                }
            }
        }
    }

    fn victory(&self) -> bool {
        for i in 0..self.rows {
            for j in 0..self.cols {
                if !self.cells[i][j].pipebomb 
                && self.cells[i][j].state != State::Open {
                    return false;
                }
            }
        }
        return true;
    }

    fn render(&self) {
        clear_term!();
        let vert = {
            let mut vert = String::new();
            for i in 0..self.cols {
                vert.push_str(" _ ");
            }
            vert
        };
        println!(" {} ", vert);
        for r in 0..self.rows {
            print!("|");
            for c in 0..self.cols {
                let cursor_here: bool = self.is_cursor_at(r, c);
                print!(
                    "{}{}{}",
                    if cursor_here { "[" } else { " " },
                    match self.cells[r][c].state {
                        State::Open => self.cell_str_at(r, c),
                        State::Closed => CLOSED.to_owned(),
                        State::Flagged => FLAGGED.to_owned(),
                    },
                    if cursor_here { "]" } else { " " }
                )
            }
            println!("|");
        }
        println_flush!(" {} ", vert);
    }
}

use std::env;
fn main1() {
    let args: Vec<String> = env::args().collect();
    dbg!(args);
}

// TODO: Add victory conditions
fn main() {
    // Set non-canonical mode:
    let og_attr = Termios::from_fd(STDIN_FILENO).unwrap();
    let mut new_attr = og_attr.clone();

    new_attr.c_lflag &= !(ICANON | ECHO);
    tcsetattr(STDIN_FILENO, TCSANOW, &mut new_attr).unwrap();
    let mut buffer = [0u8; 1]; // To read exactly one byte (key, char, etc)

    let args: Vec<String> = env::args().collect();
    let rows = args[1].parse::<usize>().unwrap_or_else(|_| 8);
    let cols = args[2].parse::<usize>().unwrap_or_else(|_| 8);
    let bomb_pcnt = args[3].parse::<usize>().unwrap_or_else(|_| 16);

    let mut main_field = Field::new(rows, cols, bomb_pcnt);

    main_field.randomize();
    main_field.render();
    let mut quit = false;
    let mut victory = false;
    let mut game_over = false;
    while !quit {
        std::io::stdin().read_exact(&mut buffer).unwrap();

        match buffer[0] as char {
            'A' | 'a' => main_field.dec_cursor(Orientation::Horizontal),
            'W' | 'w' => main_field.dec_cursor(Orientation::Vertical),
            'S' | 's' => main_field.inc_cursor(Orientation::Vertical),
            'D' | 'd' => main_field.inc_cursor(Orientation::Horizontal),
            'F' | 'f' => main_field.flag_at_cursor(),
            ' ' => {
                if main_field.open_at_cursor(&mut buffer) {
                    game_over = true
                } else {
                    main_field.check_at(main_field.cursor[0], main_field.cursor[1]);
                }
            }
            // ' ' => main_field.check_at(main_field.cursor[0], main_field.cursor[1]),
            'R' | 'r' => {
                print_flush!("{}", "\nAre you sure you want to reset? (Y/N): ");
                loop {
                    std::io::stdin().read_exact(&mut buffer).unwrap();
                    match buffer[0] as char {
                        'Y' | 'y' => {
                            main_field.randomize();
                            break;
                        }
                        'N' | 'n' => {
                            break;
                        }
                        _ => (),
                    }
                }
            }
            'Q' | 'q' => {
                print_flush!("{}", "\nAre you sure you want to quit? (Y/N): ");
                loop {
                    std::io::stdin().read_exact(&mut buffer).unwrap();
                    match buffer[0] as char {
                        'Y' | 'y' => {
                            quit = true;
                            break;
                        }
                        'N' | 'n' => {
                            break;
                        }
                        _ => (),
                    }
                }
            }
            _ => println!("??? what"),
        }
        if game_over {
            main_field.reveal_mines();
            quit = true;
        }
        if main_field.victory() {
            main_field.reveal_mines();
            victory = true;
            quit = true;
        }
        main_field.render();
    }

    if game_over {
        println!("\nWhoops!");
    } else if victory {
        println!("You won!")
    } else {
        println!("\nBye-bye!");
    }

    tcsetattr(STDIN_FILENO, TCSANOW, &og_attr).unwrap();
}
