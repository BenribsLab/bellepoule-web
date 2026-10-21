//! Tri par fusion identique à `g_list_sort` / `g_slist_sort` de GLib.
//!
//! Le tirage au sort départage deux joueurs par une comparaison qui n'est
//! pas toujours transitive (`Player::RandomCompare`). `slice::sort_by` de
//! Rust détecte ce cas et s'arrête sur une erreur ; le tri de GLib, lui,
//! produit simplement un ordre. On reproduit son algorithme pour obtenir
//! le même ordre que la version d'origine et ne jamais planter.

use std::cmp::Ordering;

/// Trie `items` de façon stable avec la fonction de comparaison fournie, qui
/// peut être incohérente sans conséquence autre qu'un ordre arbitraire.
pub fn glib_sort_by<T: Clone, F: FnMut(&T, &T) -> Ordering>(items: &mut [T], mut compare: F) {
    if items.len() < 2 {
        return;
    }
    let sorted = sort_real(items.to_vec(), &mut compare);
    items.clone_from_slice(&sorted);
}

fn sort_real<T: Clone, F: FnMut(&T, &T) -> Ordering>(mut list: Vec<T>, compare: &mut F) -> Vec<T> {
    if list.len() < 2 {
        return list;
    }
    // La première moitié contient `len / 2` éléments (cf. la double
    // progression des pointeurs dans `g_slist_sort_real`).
    let second = list.split_off(list.len() / 2);
    let first = sort_real(list, compare);
    let second = sort_real(second, compare);
    merge(first, second, compare)
}

fn merge<T, F: FnMut(&T, &T) -> Ordering>(first: Vec<T>, second: Vec<T>, compare: &mut F) -> Vec<T> {
    let mut out = Vec::with_capacity(first.len() + second.len());
    let mut a = first.into_iter().peekable();
    let mut b = second.into_iter().peekable();
    loop {
        match (a.peek(), b.peek()) {
            (Some(x), Some(y)) => {
                if compare(x, y) != Ordering::Greater {
                    out.push(a.next().unwrap());
                } else {
                    out.push(b.next().unwrap());
                }
            }
            (Some(_), None) => out.push(a.next().unwrap()),
            (None, Some(_)) => out.push(b.next().unwrap()),
            (None, None) => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_and_is_stable() {
        let mut v = vec![(3, 'a'), (1, 'b'), (3, 'c'), (2, 'd'), (1, 'e')];
        glib_sort_by(&mut v, |x, y| x.0.cmp(&y.0));
        assert_eq!(v, vec![(1, 'b'), (1, 'e'), (2, 'd'), (3, 'a'), (3, 'c')]);
    }

    #[test]
    fn inconsistent_comparator_does_not_panic() {
        // Pierre-feuille-ciseaux : pas d'ordre total.
        let mut v: Vec<u32> = (0..64).collect();
        glib_sort_by(&mut v, |a, b| ((a % 3) + 3 - (b % 3)).rem_euclid(3).cmp(&1));
        assert_eq!(v.len(), 64);
    }
}
