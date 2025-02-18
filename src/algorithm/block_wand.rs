use generator::done;

use crate::{
    segment::{
        delete::DeleteBitmapReader,
        field_norm::{id_to_fieldnorm, FieldNormRead, FieldNormReader},
        posting::{PostingCursor, TERMINATED_DOC},
    },
    utils::topk_computer::TopKComputer,
    weight::Bm25Weight,
};

pub struct SealedScorer {
    pub posting: PostingCursor,
    pub weight: Bm25Weight,
    pub max_score: f32,
}

impl SealedScorer {
    pub fn into_iter<'a>(
        self,
        fieldnorm_reader: &'a FieldNormReader,
        delete_bitmap_reader: &'a DeleteBitmapReader,
    ) -> impl Iterator<Item = (f32, u32)> + 'a {
        let mut scorer = self;
        let g = generator::Gn::new_scoped_local(move |mut s| {
            loop {
                scorer.posting.decode_block();
                loop {
                    let docid = scorer.posting.docid();
                    if !delete_bitmap_reader.is_delete(docid) {
                        let tf = scorer.posting.freq();
                        let fieldnorm_id = fieldnorm_reader.read(docid);
                        let fieldnorm = id_to_fieldnorm(fieldnorm_id);
                        let score = scorer.weight.score(fieldnorm, tf);
                        s.yield_with((score, scorer.posting.docid()));
                    }
                    if !scorer.posting.next_doc() {
                        break;
                    }
                }
                if !scorer.posting.next_block() {
                    break;
                }
            }
            done!()
        });
        g.into_iter()
    }
}

pub fn block_wand_single(
    mut scorer: SealedScorer,
    fieldnorm_reader: &FieldNormReader,
    delete_bitmap_reader: &DeleteBitmapReader,
    computer: &mut TopKComputer,
) {
    'outer: loop {
        while scorer.posting.block_max_score(&scorer.weight) <= computer.threshold() {
            if !scorer.posting.next_block() {
                break 'outer;
            }
        }
        scorer.posting.decode_block();
        loop {
            let docid = scorer.posting.docid();
            if !delete_bitmap_reader.is_delete(docid) {
                let tf = scorer.posting.freq();
                let fieldnorm_id = fieldnorm_reader.read(docid);
                let fieldnorm = id_to_fieldnorm(fieldnorm_id);
                let score = scorer.weight.score(fieldnorm, tf);
                computer.push(score, scorer.posting.docid());
            }
            if !scorer.posting.next_doc() {
                break;
            }
        }
        if !scorer.posting.next_block() {
            break;
        }
    }
}

pub fn block_wand(
    mut scorers: Vec<SealedScorer>,
    fieldnorm_reader: &FieldNormReader,
    delete_bitmap_reader: &DeleteBitmapReader,
    computer: &mut TopKComputer,
) {
    for s in &mut scorers {
        s.posting.decode_block();
    }

    // (scorer index, docid)
    let mut indexes = scorers
        .iter()
        .enumerate()
        .map(|(i, s)| (u32::try_from(i).unwrap(), s.posting.docid()))
        .collect::<Vec<_>>();
    indexes.sort_unstable_by_key(|(_, docid)| *docid);

    while let Some((before_pivot_len, pivot_len, pivot_doc)) =
        find_pivot_doc(&indexes, &scorers, computer.threshold())
    {
        let block_max_score_upperbound: f32 = indexes[..pivot_len]
            .iter()
            .map(|(i, _)| {
                let scorer = &mut scorers[*i as usize];
                scorer.posting.shallow_seek(pivot_doc);
                scorer.posting.block_max_score(&scorer.weight)
            })
            .sum();

        if block_max_score_upperbound <= computer.threshold() {
            block_max_was_too_low_advance_one_scorer(&mut indexes, &mut scorers, pivot_len);
            continue;
        }

        if !align_scorers(&mut indexes, &mut scorers, pivot_doc, before_pivot_len) {
            continue;
        }

        if !delete_bitmap_reader.is_delete(pivot_doc) {
            let len = id_to_fieldnorm(fieldnorm_reader.read(pivot_doc));
            let score = indexes[..pivot_len]
                .iter()
                .map(|(i, _)| {
                    let scorer = &scorers[*i as usize];
                    scorer.weight.score(len, scorer.posting.freq())
                })
                .sum();
            computer.push(score, pivot_doc);
        }

        advance_all_scorers_on_pivot(&mut indexes, &mut scorers, pivot_len);
    }
}

fn find_pivot_doc(
    indexes: &[(u32, u32)],
    scorers: &[SealedScorer],
    threshold: f32,
) -> Option<(usize, usize, u32)> {
    let mut max_score = 0.0;
    let mut before_pivot_len = 0;
    let mut pivot_doc = u32::MAX;
    while before_pivot_len < indexes.len() {
        max_score += scorers[indexes[before_pivot_len].0 as usize].max_score;
        if max_score > threshold {
            pivot_doc = indexes[before_pivot_len].1;
            break;
        }
        before_pivot_len += 1;
    }
    if pivot_doc == u32::MAX {
        return None;
    }

    let mut pivot_len = before_pivot_len + 1;
    pivot_len += indexes[pivot_len..]
        .iter()
        .take_while(|(_, docid)| *docid == pivot_doc)
        .count();
    Some((before_pivot_len, pivot_len, pivot_doc))
}

fn block_max_was_too_low_advance_one_scorer(
    indexes: &mut [(u32, u32)],
    scorers: &mut [SealedScorer],
    pivot_len: usize,
) {
    let mut scorer_to_seek = pivot_len - 1;
    let mut global_max_score = scorers[indexes[scorer_to_seek].0 as usize].max_score;
    let mut doc_to_seek_after = scorers[indexes[scorer_to_seek].0 as usize]
        .posting
        .last_doc_in_block();

    for scorer_ord in (0..pivot_len - 1).rev() {
        let scorer = &scorers[indexes[scorer_ord].0 as usize];
        if scorer.posting.last_doc_in_block() <= doc_to_seek_after {
            doc_to_seek_after = scorer.posting.last_doc_in_block();
        }
        if scorer.max_score > global_max_score {
            global_max_score = scorer.max_score;
            scorer_to_seek = scorer_ord;
        }
    }
    doc_to_seek_after = doc_to_seek_after.saturating_add(1);

    for (_, docid) in &indexes[pivot_len..] {
        if *docid <= doc_to_seek_after {
            doc_to_seek_after = *docid;
        }
    }
    let new_docid = scorers[indexes[scorer_to_seek].0 as usize]
        .posting
        .seek(doc_to_seek_after);
    indexes[scorer_to_seek].1 = new_docid;

    restore_ordering(indexes, scorer_to_seek);
}

fn restore_ordering(indexes: &mut [(u32, u32)], ord: usize) {
    let doc = indexes[ord].1;
    let pos = match indexes[(ord + 1)..]
        .iter()
        .position(|(_, docid)| *docid >= doc)
    {
        Some(pos) => pos,
        None => indexes.len() - ord - 1,
    };
    let tmp = indexes[ord];
    indexes.copy_within(ord + 1..ord + 1 + pos, ord);
    indexes[ord + pos] = tmp;
}

fn align_scorers(
    indexes: &mut Vec<(u32, u32)>,
    term_scorers: &mut [SealedScorer],
    pivot_doc: u32,
    before_pivot_len: usize,
) -> bool {
    for i in (0..before_pivot_len).rev() {
        let new_doc = term_scorers[indexes[i].0 as usize].posting.seek(pivot_doc);
        indexes[i].1 = new_doc;
        if new_doc != pivot_doc {
            if new_doc == TERMINATED_DOC {
                indexes.remove(i);
            } else {
                restore_ordering(indexes, i);
            }
            return false;
        }
    }
    true
}

fn advance_all_scorers_on_pivot(
    indexes: &mut Vec<(u32, u32)>,
    term_scorers: &mut [SealedScorer],
    pivot_len: usize,
) {
    for (i, docid) in &mut indexes[..pivot_len] {
        let scorer = &mut term_scorers[*i as usize];
        scorer.posting.next_with_auto_decode();
        if scorer.posting.completed() {
            *docid = TERMINATED_DOC;
        } else {
            *docid = scorer.posting.docid();
        }
    }
    indexes.sort_unstable_by_key(|(_, docid)| *docid);
    let remove_size = indexes
        .iter()
        .rev()
        .take_while(|(_, docid)| *docid == TERMINATED_DOC)
        .count();
    indexes.truncate(indexes.len() - remove_size);
}
