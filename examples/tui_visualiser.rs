//! Live TUI visualiser for the IP-range ban detection demo.
//!
//! Displays the two G-tree subtrees of the root side by side, so you can
//! watch the coordinate space split as observations arrive:
//!
//!   ┌─ 0.0.0.0–127.255.255.255  [↑↓] scroll ───┐ ┌─ 128.0.0.0–255.255.255.255 ────────────┐
//!   │ [d1] 0.0.0.0–127.255.255.255  own=1 (I)  │ │ [d1] 128.0.0.0–255.255.255.255 (I)     │
//!   │ └── [d2] 64.0.0.0–127.255.255.255  (T)   │ │ └── [d5] 203.0.113.0–203.0.113.255     │
//!   └──────────────────────────────────────────┘ └────────────────────────────────────────┘
//!   ┌─ /24 density  203.0.x.0  (each bar = 4 subnets) ────────────────────────────────────┐
//!   └─────────────────────────────────────────────────────────────────────────────────────┘
//!   ┌─ Phase: …  total=5120  [q] quit  [↑↓] scroll … ─────────────────────────────────────┐
//!   └─────────────────────────────────────────────────────────────────────────────────────┘
//!
//! Before the root has been split both panels show the single root node so
//! the screen is never blank.
//!
//! Colours (by node `sum` relative to `total_sum()`):
//!   red+bold  > 60 %   — hot zone (the attack)
//!   yellow    > 20 %   — elevated
//!   cyan      >  0 %   — some activity
//!   white           =  0 %   — silent (visible skeleton)
//!
//! Controls:
//!   `q` / `Esc`     — quit early
//!   `↑` / `↓`       — scroll both tree panels simultaneously
//!   `Home` / `End`  — jump to top / bottom of both panels
//!   Any other key   — skip the current wait and advance immediately
//!
//! Run with:
//! ```bash
//! cargo run --example tui_visualiser
//! ```

use std::collections::HashMap;
use std::io;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Borders, Paragraph};
use torrust_mudlark::{Config, GNodeId, GState, GvGraph};

// ---------------------------------------------------------------------------
// Graph type alias
// ---------------------------------------------------------------------------

type BadRequestMap = GvGraph<u32, u64, 32>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ipv4(a: u8, b: u8, c: u8, d: u8) -> u32 {
    u32::from(Ipv4Addr::new(a, b, c, d))
}

// ---------------------------------------------------------------------------
// G-tree DFS renderer
// ---------------------------------------------------------------------------

/// Recursively walk the G-tree from `id` using the public
/// `gnode_info()` + `gnode_children()` API, emitting box-drawing lines.
///
/// * `prefix`       — indentation inherited from the parent row
/// * `connector`    — `├── ` / `└── ` (or `""` for the root)
/// * `child_prefix` — indentation to pass down to children
fn visit_gnode(
    graph: &BadRequestMap,
    id: GNodeId,
    prefix: String,
    connector: &'static str,
    child_prefix: String,
    peak_sum: u64,
    lines: &mut Vec<Line<'static>>,
) {
    let Some(node) = graph.gnode_info(id) else {
        return;
    };

    let s = Ipv4Addr::from(node.start).to_string();
    let e = Ipv4Addr::from(node.end).to_string();
    let state_str = match node.state {
        GState::Terminal => "T",
        GState::Internal => "I",
        GState::SemiInternal => "S",
    };

    let frac = node.sum as f64 / peak_sum.max(1) as f64;
    let (fg, modif) = if frac > 0.6 {
        (Color::Red, Modifier::BOLD)
    } else if frac > 0.2 {
        (Color::Yellow, Modifier::empty())
    } else if node.sum > 0 {
        (Color::Cyan, Modifier::empty())
    } else {
        // Use visible Gray instead of DarkGray so the tree skeleton never
        // disappears on a dark background.
        (Color::Gray, Modifier::empty())
    };

    lines.push(Line::from(vec![
        Span::styled(
            format!("{prefix}{connector}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!(
                "[d{}] {}–{}  own={}  sum={}  ({})",
                node.depth, s, e, node.own, node.sum, state_str
            ),
            Style::default().fg(fg).add_modifier(modif),
        ),
    ]));

    if let Some(ch) = graph.gnode_children(id) {
        match (ch.left, ch.right) {
            (Some(left), Some(right)) => {
                visit_gnode(
                    graph,
                    left,
                    child_prefix.clone(),
                    "├── ",
                    format!("{child_prefix}│   "),
                    peak_sum,
                    lines,
                );
                visit_gnode(
                    graph,
                    right,
                    child_prefix.clone(),
                    "└── ",
                    format!("{child_prefix}    "),
                    peak_sum,
                    lines,
                );
            }
            (Some(only), None) | (None, Some(only)) => {
                visit_gnode(
                    graph,
                    only,
                    child_prefix.clone(),
                    "└── ",
                    format!("{child_prefix}    "),
                    peak_sum,
                    lines,
                );
            }
            (None, None) => {}
        }
    }
}

/// Build styled lines for the **full G-tree** (all allocated G-nodes) from
/// `g_root()`.  Empty/inactive nodes are shown in Gray, active ones coloured
/// by their `sum` relative to `total_sum()`.
fn build_gtree_lines(graph: &BadRequestMap) -> Vec<Line<'static>> {
    let peak_sum = graph.total_sum();
    let mut lines = Vec::new();
    visit_gnode(
        graph,
        graph.g_root(),
        String::new(),
        "",
        String::new(),
        peak_sum,
        &mut lines,
    );
    lines
}

// ---------------------------------------------------------------------------
// V-tree renderer (active nodes via layers())
// ---------------------------------------------------------------------------

/// Build styled lines for the **V-tree** — only the G-nodes that are live
/// (have a V-tree Entry), as returned by `graph.layers()`.
///
/// `layers()` skips V-tree Structural nodes and only yields Entry nodes
/// carrying their G-tree parent ID.  We rebuild the hierarchy from those
/// parent links: nodes whose G-tree parent is *not* itself active appear
/// as top-level roots in this view.
fn build_vtree_lines(graph: &BadRequestMap) -> Vec<Line<'static>> {
    let peak_sum = graph.total_sum();

    // Collect all active nodes.
    struct Flat {
        id: GNodeId,
        parent: Option<GNodeId>,
        start: u32,
        end: u32,
        own: u64,
        sum: u64,
        depth: u32,
        state: GState,
    }
    let flat: Vec<Flat> = graph
        .layers()
        .map(|(_bfs_depth, node)| Flat {
            id: node.gnode_id,
            parent: node.parent,
            start: node.start,
            end: node.end,
            own: node.own,
            sum: node.sum,
            depth: node.depth,
            state: node.state,
        })
        .collect();

    // Build id → index map and parent → children map.
    let mut id_to_idx: HashMap<GNodeId, usize> = HashMap::new();
    for (i, n) in flat.iter().enumerate() {
        id_to_idx.insert(n.id, i);
    }
    let mut children_of: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (i, n) in flat.iter().enumerate() {
        match n.parent {
            Some(pid) => match id_to_idx.get(&pid) {
                Some(&pi) => children_of.entry(pi).or_default().push(i),
                None => roots.push(i),
            },
            None => roots.push(i),
        }
    }

    if flat.is_empty() {
        return vec![Line::from(Span::styled(
            "  (no active nodes yet)",
            Style::default().fg(Color::DarkGray),
        ))];
    }

    // Recursive DFS closure — use a standalone nested fn to avoid lifetime
    // issues with recursive closures.
    fn dfs(
        flat: &[Flat],
        children_of: &HashMap<usize, Vec<usize>>,
        idx: usize,
        prefix: String,
        connector: &'static str,
        child_prefix: String,
        peak_sum: u64,
        lines: &mut Vec<Line<'static>>,
    ) {
        let n = &flat[idx];
        let s = Ipv4Addr::from(n.start).to_string();
        let e = Ipv4Addr::from(n.end).to_string();
        let state_str = match n.state {
            GState::Terminal => "T",
            GState::Internal => "I",
            GState::SemiInternal => "S",
        };
        let frac = n.sum as f64 / peak_sum.max(1) as f64;
        let (fg, modif) = if frac > 0.6 {
            (Color::Red, Modifier::BOLD)
        } else if frac > 0.2 {
            (Color::Yellow, Modifier::empty())
        } else if n.sum > 0 {
            (Color::Cyan, Modifier::empty())
        } else {
            (Color::Gray, Modifier::empty())
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{prefix}{connector}"),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!(
                    "[d{}] {}–{}  own={}  sum={}  ({})",
                    n.depth, s, e, n.own, n.sum, state_str
                ),
                Style::default().fg(fg).add_modifier(modif),
            ),
        ]));
        let kids = children_of.get(&idx).cloned().unwrap_or_default();
        let n_kids = kids.len();
        for (i, ci) in kids.into_iter().enumerate() {
            let is_last = i == n_kids - 1;
            let (cc, cp): (&'static str, String) = if is_last {
                ("└── ", format!("{child_prefix}    "))
            } else {
                ("├── ", format!("{child_prefix}│   "))
            };
            dfs(
                flat,
                children_of,
                ci,
                child_prefix.clone(),
                cc,
                cp,
                peak_sum,
                lines,
            );
        }
    }

    let mut lines = Vec::new();
    let n_roots = roots.len();
    for (i, ri) in roots.into_iter().enumerate() {
        let is_last = i == n_roots - 1;
        if n_roots == 1 {
            dfs(
                &flat,
                &children_of,
                ri,
                String::new(),
                "",
                String::new(),
                peak_sum,
                &mut lines,
            );
        } else if is_last {
            dfs(
                &flat,
                &children_of,
                ri,
                String::new(),
                "└── ",
                "    ".to_string(),
                peak_sum,
                &mut lines,
            );
        } else {
            dfs(
                &flat,
                &children_of,
                ri,
                String::new(),
                "├── ",
                "│   ".to_string(),
                peak_sum,
                &mut lines,
            );
        }
    }
    lines
}

// ---------------------------------------------------------------------------
// Heat-map bar data
// ---------------------------------------------------------------------------

fn build_heatmap_bars(graph: &BadRequestMap, a: u8, b: u8) -> Vec<u64> {
    (0_u8..=255)
        .map(|c| {
            let lo = u32::from(Ipv4Addr::new(a, b, c, 0));
            let hi = u32::from(Ipv4Addr::new(a, b, c, 255));
            graph.range_sum(lo..=hi)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

struct AppState {
    /// Scroll offset applied to both tree panels simultaneously.
    scroll: u16,
}

// ---------------------------------------------------------------------------
// Event handling
// ---------------------------------------------------------------------------

/// Wait up to `timeout` for a key press, handling scroll keys inline.
///
/// Returns `true` if the user wants to quit (`q`/`Esc`), `false` on any other
/// key or on timeout.  `↑`/`↓`/`Home`/`End` scroll both panels without
/// consuming the "advance" signal.
fn advance_or_scroll(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    graph: &BadRequestMap,
    state: &mut AppState,
    phase: &str,
    timeout: Duration,
) -> io::Result<bool> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        if !event::poll(remaining.min(Duration::from_millis(50)))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
                KeyCode::Up => {
                    state.scroll = state.scroll.saturating_sub(3);
                    render(terminal, graph, state, phase, graph.total_sum())?;
                }
                KeyCode::Down => {
                    state.scroll = state.scroll.saturating_add(3);
                    render(terminal, graph, state, phase, graph.total_sum())?;
                }
                KeyCode::Home => {
                    state.scroll = 0;
                    render(terminal, graph, state, phase, graph.total_sum())?;
                }
                KeyCode::End => {
                    // 9999 is a safe large value; ratatui caps at actual content.
                    state.scroll = 9999;
                    render(terminal, graph, state, phase, graph.total_sum())?;
                }
                _ => return Ok(false),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

fn render(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    graph: &BadRequestMap,
    state: &AppState,
    phase: &str,
    total: u64,
) -> io::Result<()> {
    // Left panel  = full G-tree (all allocated G-nodes, geometric partition)
    // Right panel = V-tree  (only the live/active nodes returned by layers())
    let left_lines = build_gtree_lines(graph);
    let right_lines = build_vtree_lines(graph);

    let bars = build_heatmap_bars(graph, 203, 0);
    let peak = bars.iter().copied().max().unwrap_or(1).max(1);

    terminal.draw(|frame| {
        let area = frame.area();

        // ── Three vertical rows: trees | heatmap | status ─────────────────
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // tree panels
                Constraint::Length(5), // /24 heatmap strip
                Constraint::Length(3), // status bar
            ])
            .split(area);

        // ── Two equal tree panels ──────────────────────────────────────────
        let tree_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0]);

        let left_title = " G-tree (all nodes) [↑↓] scroll ";
        let right_title = " V-tree (active/live nodes) ";

        let left_widget = Paragraph::new(left_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(left_title)
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
            )
            .scroll((state.scroll, 0));
        frame.render_widget(left_widget, tree_cols[0]);

        let right_widget = Paragraph::new(right_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(right_title)
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
            )
            .scroll((state.scroll, 0));
        frame.render_widget(right_widget, tree_cols[1]);

        // ── /24 heatmap strip (bottom) ─────────────────────────────────────
        // Group 256 /24 subnets (203.0.0.0–203.0.255.0) into 64 bars of 4.
        const GROUP: usize = 4;
        let col_vals: Vec<u64> = (0..64_usize)
            .map(|c| (0..GROUP).map(|i| bars[c * GROUP + i]).sum())
            .collect();

        let bar_defs: Vec<Bar<'_>> = col_vals
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let frac = v as f64 / peak as f64;
                let colour = if frac > 0.8 {
                    Color::Red
                } else if frac > 0.5 {
                    Color::Yellow
                } else if frac > 0.2 {
                    Color::Cyan
                } else if frac > 0.0 {
                    Color::Blue
                } else {
                    Color::DarkGray
                };
                // Label every 16th bar (= every 64 subnets) with its start address.
                Bar::default()
                    .value(v)
                    .style(Style::default().fg(colour))
                    .value_style(Style::default().fg(Color::Reset))
                    .text_value(String::new())
                    .label(if i % 16 == 0 {
                        Line::from(Span::styled(
                            format!("203.0.{}.0", i * GROUP),
                            Style::default().fg(Color::DarkGray),
                        ))
                    } else {
                        Line::from("")
                    })
            })
            .collect();

        let bar_group = BarGroup::default().bars(&bar_defs);
        let chart = BarChart::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" /24 density  203.0.x.0  (each bar = 4 subnets) ")
                    .title_style(
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
            )
            .data(bar_group)
            .bar_width(1)
            .bar_gap(0)
            .max(peak);
        frame.render_widget(chart, rows[1]);

        // ── Status bar ─────────────────────────────────────────────────────
        let status = Paragraph::new(Line::from(vec![
            Span::styled(" Phase: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                phase,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   total={total}"),
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                "   [q] quit  [↑↓] scroll  [Home] top  [End] bottom  [key] skip",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL));
        frame.render_widget(status, rows[2]);
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let step_delay = Duration::from_millis(500);
    let attack_delay = Duration::from_millis(80);
    let phase_pause = Duration::from_secs(2);

    let mut graph = BadRequestMap::new(Config {
        split_threshold: 5,
        depth_create: 4,
        depth_evict: 16,
        budget: None,
        alpha_relax: 0.5,
        bounded_eviction: false,
    });

    let mut state = AppState { scroll: 0 };

    // Render the current graph state, then wait for a key/timeout.
    // Returns early if the user quits.
    macro_rules! show {
        ($phase:expr, $delay:expr) => {{
            render(terminal, &graph, &state, $phase, graph.total_sum())?;
            if advance_or_scroll(terminal, &graph, &mut state, $phase, $delay)? {
                return Ok(());
            }
        }};
    }

    // -- 1. Background noise: three IPs from unrelated ranges -----------------
    for &(a, b, c, d) in &[
        (10_u8, 0_u8, 0_u8, 1_u8),
        (192, 168, 1, 42),
        (172, 16, 5, 100),
    ] {
        graph.observe(ipv4(a, b, c, d), 1_u64);
        show!("Phase 1 — noise (3 isolated IPs)", step_delay);
    }
    show!("Phase 1 — noise complete", phase_pause);

    // -- 2. Coordinated attack: every host in 203.0.113.0/24 ------------------
    for host in 0_u8..=255 {
        for _ in 0..20_u32 {
            graph.observe(ipv4(203, 0, 113, host), 1_u64);
        }
        // Render every 8 hosts.
        if host % 8 == 7 || host == 255 {
            let phase_label = format!(
                "Phase 2 — attack: 203.0.113.0–{host}  ({} hosts done)",
                u16::from(host) + 1
            );
            show!(&phase_label, attack_delay);
        }
    }
    show!(
        "Phase 2 — attack complete  (5 120 bad reqs in /24)",
        phase_pause
    );

    // -- 3. Pause to inspect the final tree -----------------------------------
    show!("Phase 3 — inspect  range_sum(203.0.113.0/24)", phase_pause);

    // -- 4. Decay: halve all counts -------------------------------------------
    let root = graph.g_root();
    graph.decay(root, 0.5, 0.001);
    show!("Phase 4 — decay ×0.5  (counts halved)", phase_pause);

    // Stay on screen until the user quits.
    loop {
        render(
            terminal,
            &graph,
            &state,
            "Done — press q to quit",
            graph.total_sum(),
        )?;
        if advance_or_scroll(
            terminal,
            &graph,
            &mut state,
            "Done — press q to quit",
            Duration::from_millis(100),
        )? {
            break;
        }
    }

    Ok(())
}
