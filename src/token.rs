use std::collections::HashSet;

use tocken::tokenizer::Tokenizer as Tockenizer;
use unicode_segmentation::UnicodeSegmentation;

use crate::guc::TOKENIZER_NAME;

static BERT_BASE_UNCASED_BYTES: &[u8] = include_bytes!("../tokenizer/bert_base_uncased.json");
static TOCKEN: &[u8] = include_bytes!("../tokenizer/wiki_tocken.json");

const TOKEN_PATTERN: &str = r"(?u)\b\w\w+\b";

lazy_static::lazy_static! {
    static ref TOKEN_PATTERN_RE: regex::Regex = regex::Regex::new(TOKEN_PATTERN).unwrap();
    pub static ref STOP_WORDS_LUCENE: HashSet<String> = {
        [
            "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is",
            "it", "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there",
            "these", "they", "this", "to", "was", "will", "with",
        ].iter().map(|s| s.to_string()).collect()
    };
    pub static ref STOP_WORDS_NLTK: HashSet<String> = {
        let words = stop_words::get(stop_words::LANGUAGE::English);
        words.into_iter().collect()
    };

    static ref BERT_TOKENIZER: BertWithStemmerAndSplit = Default::default();
    static ref TOCKENIZER: Tocken = Tocken(Tockenizer::loads(std::str::from_utf8(TOCKEN).expect("str")));
}

pub fn tokenize(text: &str) -> Vec<u32> {
    match TOKENIZER_NAME
        .get()
        .expect("set guc")
        .to_str()
        .expect("str")
    {
        "BERT" => BERT_TOKENIZER.encode(text),
        "TOCKEN" => TOCKENIZER.encode(text),
        "UNICODE" => panic!("only support the trigger"),
        _ => panic!("Unknown tokenizer"),
    }
}

pub fn unicode_tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for word in text.unicode_words() {
        // trim `'s` for English
        let mut lowercase = word.to_lowercase();
        if lowercase.len() >= 2 && lowercase.ends_with("s") {
            let chars = lowercase.chars().collect::<Vec<char>>();
            let c = chars[chars.len() - 2];
            if c == '\'' || c == '\u{2019}' || c == '\u{FF07}' {
                lowercase = chars[..chars.len() - 2].iter().collect::<String>();
            }
        }
        let token = tantivy_stemmers::algorithms::english_porter(&lowercase).to_string();
        if token.is_empty() {
            continue;
        }
        if !STOP_WORDS_LUCENE.contains(&lowercase) {
            tokens.push(token.clone());
        }
        if !STOP_WORDS_NLTK.contains(&lowercase) {
            tokens.push(token);
        }
    }
    tokens
}

trait Tokenizer {
    fn encode(&self, text: &str) -> Vec<u32>;
}

struct BertWithStemmerAndSplit(tokenizers::Tokenizer);

impl Default for BertWithStemmerAndSplit {
    fn default() -> Self {
        Self(tokenizers::Tokenizer::from_bytes(BERT_BASE_UNCASED_BYTES).unwrap())
    }
}

impl Tokenizer for BertWithStemmerAndSplit {
    fn encode(&self, text: &str) -> Vec<u32> {
        let mut results = Vec::new();
        let lower_text = text.to_lowercase();
        let split = TOKEN_PATTERN_RE.find_iter(&lower_text);
        for token in split {
            if STOP_WORDS_NLTK.contains(token.as_str()) {
                continue;
            }
            let stemmed_token =
                tantivy_stemmers::algorithms::english_porter_2(token.as_str()).to_string();
            let encoding = self.0.encode_fast(stemmed_token, false).unwrap();
            results.extend_from_slice(encoding.get_ids());
        }
        results
    }
}

struct Tocken(Tockenizer);

impl Tokenizer for Tocken {
    fn encode(&self, text: &str) -> Vec<u32> {
        self.0.tokenize(text)
    }
}
