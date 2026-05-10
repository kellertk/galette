//
// optimize.rs: Quine-McCluskey based equation optimizer.
//
// Wraps the `quine_mc_cluskey` crate to minimize a `gal::Term`,
// optionally trying both polarities, and converts results back to the
// `parser::Equation` form for display and write-back.
//

use std::collections::BTreeSet;

use quine_mc_cluskey::Bool;

use crate::{
    chips::Chip,
    errors::LineNum,
    gal::{self, Pin, Term},
    parser::{Equation, LHS, Suffix},
};

#[derive(Clone, Debug)]
pub struct Optimized {
    pub term: Term,
    pub flipped_polarity: bool,
    pub products_before: usize,
    pub products_after: usize,
    pub literals_before: usize,
    pub literals_after: usize,
}

// Returns Some only when strictly better than the input on (products,
// literals). `allow_flip = false` for AR/SP (no polarity).
pub fn optimize_term(term: &Term, allow_flip: bool) -> Option<Optimized> {
    if is_true(term) || is_false(term) {
        return None;
    }

    let vars = unique_vars(term);
    if vars.is_empty() {
        return None;
    }

    let (p_in, l_in) = count_pl(term);

    let bool_in = term_to_bool(term, &vars);
    let bool_orig = pick_simplified(&bool_in);
    let term_orig = bool_to_term(&bool_orig, &vars, term.line_num);
    let (p_orig, l_orig) = count_pl(&term_orig);

    let flip_candidate = if allow_flip {
        let bool_flipped = pick_simplified(&Bool::Not(Box::new(bool_in)));
        let term_flipped = bool_to_term(&bool_flipped, &vars, term.line_num);
        let (p_flip, l_flip) = count_pl(&term_flipped);
        Some((term_flipped, p_flip, l_flip))
    } else {
        None
    };

    enum Choice {
        Input,
        OrigPolarity,
        Flipped,
    }
    // Input wins all ties (avoids no-op suggestions); orig-polarity
    // wins flip ties (keeps LHS sign stable).
    let mut best = Choice::Input;
    let mut best_pl = (p_in, l_in);

    if (p_orig, l_orig) < best_pl {
        best = Choice::OrigPolarity;
        best_pl = (p_orig, l_orig);
    }
    if let Some((_, p_flip, l_flip)) = &flip_candidate
        && (*p_flip, *l_flip) < best_pl
    {
        best = Choice::Flipped;
        best_pl = (*p_flip, *l_flip);
    }

    match best {
        Choice::Input => None,
        Choice::OrigPolarity => Some(Optimized {
            term: term_orig,
            flipped_polarity: false,
            products_before: p_in,
            products_after: best_pl.0,
            literals_before: l_in,
            literals_after: best_pl.1,
        }),
        Choice::Flipped => {
            let (term_flipped, _, _) = flip_candidate.unwrap();
            Some(Optimized {
                term: term_flipped,
                flipped_polarity: true,
                products_before: p_in,
                products_after: best_pl.0,
                literals_before: l_in,
                literals_after: best_pl.1,
            })
        }
    }
}

// Render an Equation in galasm syntax. Negation rule: pin_names[i]
// already includes a leading `/` for declared-negative pins, so we
// display `/` iff `pin.neg ^ declared_neg` (matches lookup_pin's XOR).
pub fn format_equation(eqn: &Equation, pin_names: &[String]) -> String {
    let mut out = String::new();

    match &eqn.lhs {
        LHS::Pin((pin, suffix)) => {
            out.push_str(&display_pin(pin, pin_names));
            out.push_str(suffix_str(*suffix));
        }
        LHS::Ar => out.push_str("AR"),
        LHS::Sp => out.push_str("SP"),
    }

    out.push_str(" = ");

    if eqn.rhs.is_empty() {
        return out;
    }

    for (i, (pin, &is_or)) in eqn.rhs.iter().zip(eqn.is_or.iter()).enumerate() {
        if i > 0 {
            out.push_str(if is_or { " + " } else { " * " });
        }
        out.push_str(&display_pin(pin, pin_names));
    }

    out
}

fn display_pin(pin: &Pin, pin_names: &[String]) -> String {
    let raw = &pin_names[pin.pin - 1];
    let (declared_neg, bare) = if let Some(rest) = raw.strip_prefix('/') {
        (true, rest)
    } else {
        (false, raw.as_str())
    };
    if pin.neg ^ declared_neg {
        format!("/{bare}")
    } else {
        bare.to_string()
    }
}

fn suffix_str(s: Suffix) -> &'static str {
    match s {
        Suffix::None => "",
        Suffix::T => ".T",
        Suffix::R => ".R",
        Suffix::E => ".E",
        Suffix::CLK => ".CLK",
        Suffix::APRST => ".APRST",
        Suffix::ARST => ".ARST",
    }
}

// `chip` is needed to encode constant terms as VCC/GND pin references.
pub fn term_to_equation(
    term: &Term,
    lhs: LHS,
    chip: Chip,
    line_num: LineNum,
    end_line: LineNum,
) -> Equation {
    if is_true(term) {
        return Equation {
            line_num,
            end_line,
            lhs,
            rhs: vec![Pin {
                pin: chip.num_pins(),
                neg: false,
            }],
            is_or: vec![false],
        };
    }
    if is_false(term) {
        return Equation {
            line_num,
            end_line,
            lhs,
            rhs: vec![Pin {
                pin: chip.num_pins() / 2,
                neg: false,
            }],
            is_or: vec![false],
        };
    }

    let mut rhs = Vec::new();
    let mut is_or = Vec::new();
    for (p_idx, product) in term.pins.iter().enumerate() {
        if product.is_empty() {
            continue;
        }
        for (l_idx, lit) in product.iter().enumerate() {
            let opens_or_group = p_idx > 0 && l_idx == 0;
            rhs.push(*lit);
            is_or.push(opens_or_group);
        }
    }

    Equation {
        line_num,
        end_line,
        lhs,
        rhs,
        is_or,
    }
}

pub fn flip_lhs_polarity(lhs: &LHS) -> LHS {
    match lhs {
        LHS::Pin((pin, suffix)) => LHS::Pin((
            Pin {
                pin: pin.pin,
                neg: !pin.neg,
            },
            *suffix,
        )),
        LHS::Ar | LHS::Sp => panic!("flip_lhs_polarity called on AR/SP"),
    }
}

fn is_true(term: &Term) -> bool {
    term.pins.len() == 1 && term.pins[0].is_empty()
}

fn is_false(term: &Term) -> bool {
    term.pins.is_empty()
}

fn count_pl(term: &Term) -> (usize, usize) {
    let products = term.pins.len();
    let literals: usize = term.pins.iter().map(|p| p.len()).sum();
    (products, literals)
}

// Bool::minterms() asserts a contiguous 0..N variable naming, so pin
// numbers must be remapped to dense indices via this Vec.
fn unique_vars(term: &Term) -> Vec<usize> {
    let mut set = BTreeSet::new();
    for product in &term.pins {
        for pin in product {
            set.insert(pin.pin);
        }
    }
    set.into_iter().collect()
}

fn idx_of(vars: &[usize], pin: usize) -> u8 {
    vars.iter().position(|&p| p == pin).unwrap() as u8
}

fn term_to_bool(term: &Term, vars: &[usize]) -> Bool {
    if term.pins.is_empty() {
        return Bool::False;
    }
    let products: Vec<Bool> = term
        .pins
        .iter()
        .map(|product| {
            if product.is_empty() {
                Bool::True
            } else {
                let literals: Vec<Bool> =
                    product.iter().map(|pin| pin_to_bool(pin, vars)).collect();
                wrap_and(literals)
            }
        })
        .collect();
    wrap_or(products)
}

// Bool::And / Bool::Or require >= 2 elements.
fn wrap_and(mut items: Vec<Bool>) -> Bool {
    if items.len() == 1 {
        items.pop().unwrap()
    } else {
        Bool::And(items)
    }
}
fn wrap_or(mut items: Vec<Bool>) -> Bool {
    if items.len() == 1 {
        items.pop().unwrap()
    } else {
        Bool::Or(items)
    }
}

fn pin_to_bool(pin: &Pin, vars: &[usize]) -> Bool {
    let term = Bool::Term(idx_of(vars, pin.pin));
    if pin.neg {
        Bool::Not(Box::new(term))
    } else {
        term
    }
}

// simplify() returns equally-minimal forms; per its contract any one
// has the same product/literal count.
fn pick_simplified(b: &Bool) -> Bool {
    let mut forms = b.simplify();
    forms.swap_remove(0)
}

fn bool_to_term(b: &Bool, vars: &[usize], line_num: LineNum) -> Term {
    match b {
        Bool::True => gal::true_term(line_num),
        Bool::False => gal::false_term(line_num),
        Bool::Term(_) | Bool::Not(_) | Bool::And(_) => {
            let product = bool_to_product(b, vars);
            Term {
                line_num,
                pins: vec![product],
            }
        }
        Bool::Or(items) => {
            let products: Vec<Vec<Pin>> =
                items.iter().map(|item| bool_to_product(item, vars)).collect();
            Term {
                line_num,
                pins: products,
            }
        }
    }
}

fn bool_to_product(b: &Bool, vars: &[usize]) -> Vec<Pin> {
    match b {
        Bool::Term(i) => vec![Pin {
            pin: vars[*i as usize],
            neg: false,
        }],
        Bool::Not(inner) => match inner.as_ref() {
            Bool::Term(i) => vec![Pin {
                pin: vars[*i as usize],
                neg: true,
            }],
            other => {
                debug_assert!(false, "unexpected Not inside product: {other:?}");
                Vec::new()
            }
        },
        Bool::And(items) => items
            .iter()
            .map(|lit| {
                let p = bool_to_product(lit, vars);
                debug_assert_eq!(p.len(), 1, "And literal must yield single pin");
                p.into_iter().next().unwrap()
            })
            .collect(),
        other => {
            debug_assert!(false, "unexpected shape inside Or: {other:?}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pin: usize, neg: bool) -> Pin {
        Pin { pin, neg }
    }

    fn t(pins: Vec<Vec<(usize, bool)>>) -> Term {
        Term {
            line_num: 0,
            pins: pins
                .into_iter()
                .map(|prod| prod.into_iter().map(|(pin, neg)| p(pin, neg)).collect())
                .collect(),
        }
    }

    #[test]
    fn absorbs_x_and_x_y() {
        // A + A*B → A
        let term = t(vec![vec![(2, false)], vec![(2, false), (3, false)]]);
        let opt = optimize_term(&term, false).expect("should reduce");
        assert_eq!(opt.products_after, 1);
        assert_eq!(opt.literals_after, 1);
        assert!(!opt.flipped_polarity);
        assert_eq!(opt.term.pins, vec![vec![p(2, false)]]);
    }

    #[test]
    fn merges_complement_pair() {
        // A*B + A*/B → A
        let term = t(vec![
            vec![(2, false), (3, false)],
            vec![(2, false), (3, true)],
        ]);
        let opt = optimize_term(&term, false).expect("should reduce");
        assert_eq!(opt.term.pins, vec![vec![p(2, false)]]);
    }

    #[test]
    fn consensus_drops_redundant_term() {
        // A*B + /A*C + B*C → A*B + /A*C
        let term = t(vec![
            vec![(2, false), (3, false)],
            vec![(2, true), (4, false)],
            vec![(3, false), (4, false)],
        ]);
        let opt = optimize_term(&term, false).expect("should reduce");
        assert_eq!(opt.products_after, 2);
        assert_eq!(opt.literals_after, 4);
    }

    #[test]
    fn already_minimal_returns_none() {
        let term = t(vec![vec![(2, false)], vec![(3, false)]]);
        assert!(optimize_term(&term, false).is_none());
    }

    #[test]
    fn constants_return_none() {
        assert!(optimize_term(&gal::true_term(0), true).is_none());
        assert!(optimize_term(&gal::false_term(0), true).is_none());
    }

    #[test]
    fn polarity_flip_chosen_when_strictly_smaller() {
        // f = A*B + A*C + A*D + A*E (4 products, 8 literals)
        // !f = !A + !B*!C*!D*!E (2 products, 5 literals)
        let term = t(vec![
            vec![(2, false), (3, false)],
            vec![(2, false), (4, false)],
            vec![(2, false), (5, false)],
            vec![(2, false), (6, false)],
        ]);
        let opt = optimize_term(&term, true).expect("should reduce via flip");
        assert!(opt.flipped_polarity);
        assert_eq!(opt.products_after, 2);
    }

    #[test]
    fn format_equation_round_trips_negation_xor() {
        use crate::parser::Suffix;
        // /B is declared-negative; using it as `B` flips Pin.neg=true.
        let pin_names = vec![
            "Y".to_string(),
            "A".to_string(),
            "/B".to_string(),
            "VCC".to_string(),
        ];
        let eqn = Equation {
            line_num: 1,
            end_line: 1,
            lhs: LHS::Pin((p(1, false), Suffix::None)),
            rhs: vec![p(2, false), p(3, true)],
            is_or: vec![false, false],
        };
        assert_eq!(format_equation(&eqn, &pin_names), "Y = A * B");
    }

    #[test]
    fn format_equation_negative_lhs() {
        use crate::parser::Suffix;
        let pin_names = vec!["Y".to_string(), "A".to_string()];
        let eqn = Equation {
            line_num: 1,
            end_line: 1,
            lhs: LHS::Pin((p(1, true), Suffix::R)),
            rhs: vec![p(2, false)],
            is_or: vec![false],
        };
        assert_eq!(format_equation(&eqn, &pin_names), "/Y.R = A");
    }
}
