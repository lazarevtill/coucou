// Builds the transcript of a call from the two recordings: the microphone is
// you, the computer's sound is the others, told apart by the diarizer. Port of
// NoteBuilder (NoteModels.swift); nothing here touches the OS.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::flow::text::new_id;

/// Someone in a call. You are always "me"; the others get "Person 2",
/// "Person 3"… until renamed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Speaker {
    pub id: String,
    pub name: String,
    /// Index in the window's speaker colours.
    pub color: usize,
    #[serde(default)]
    pub is_me: bool,
}

/// One turn of speech.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    #[serde(default = "new_id")]
    pub id: String,
    pub speaker: String,
    /// Seconds from the start of the recording.
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Todo {
    #[serde(default = "new_id")]
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub done: bool,
}

/// A word with its place in the audio.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// A stretch of audio where one voice speaks, from the diarizer.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub speaker: String,
    pub start: f64,
    pub end: f64,
}

/// Words further apart than this start a new line.
pub const PAUSE: f64 = 1.4;
/// A line never gets longer than this, so the transcript stays readable.
pub const MAX_LINE_SECONDS: f64 = 40.0;
pub const ME: &str = "me";

fn distance(t: f64, turn: &Turn) -> f64 {
    if t < turn.start {
        turn.start - t
    } else if t > turn.end {
        t - turn.end
    } else {
        0.0
    }
}

/// Match the Mac's fragment handling. Sherpa exposes no per-turn embedding,
/// so brief orphan voices use the nearest established turn in time.
pub fn merging_fragments(turns: Vec<Turn>) -> Vec<Turn> {
    let mut durations: HashMap<&str, (f64, f64)> = HashMap::new();
    for turn in &turns {
        let length = (turn.end - turn.start).max(0.0);
        let entry = durations.entry(&turn.speaker).or_default();
        entry.0 += length;
        entry.1 = entry.1.max(length);
    }
    let all: f64 = durations.values().map(|(total, _)| total).sum();
    let solid: std::collections::HashSet<&str> = durations.into_iter()
        .filter(|(_, (total, longest))| *longest >= 3.0 || *total >= 20.0_f64.max(all * 0.1))
        .map(|(speaker, _)| speaker).collect();
    if solid.is_empty() { return turns; }
    turns.iter().map(|turn| {
        let mut result = turn.clone();
        if !solid.contains(turn.speaker.as_str()) {
            let mid = (turn.start + turn.end) / 2.0;
            if let Some(nearest) = turns.iter().filter(|t| solid.contains(t.speaker.as_str()))
                .min_by(|a, b| distance(mid, a).total_cmp(&distance(mid, b))) {
                result.speaker = nearest.speaker.clone();
            }
        }
        result
    }).collect()
}

/// As on Mac, giving two remote speakers the same name combines their lines.
/// The microphone identity remains independent even if a participant shares its name.
pub fn rename_speaker(speakers: &mut Vec<Speaker>, lines: &mut [Line], id: &str, name: &str) {
    let name = name.trim();
    let Some(index) = speakers.iter().position(|s| s.id == id) else { return };
    if name.is_empty() { return; }
    let other = (!speakers[index].is_me).then(|| speakers.iter()
        .find(|s| s.id != id && !s.is_me && s.name.to_lowercase() == name.to_lowercase())
        .map(|s| s.id.clone())).flatten();
    if let Some(other) = other {
        for line in lines.iter_mut().filter(|line| line.speaker == id) { line.speaker = other.clone(); }
        speakers.remove(index);
    } else {
        speakers[index].name = name.to_string();
    }
}

/// The others' words, each given to the voice speaking at that moment (or the
/// nearest one).
pub fn assign(words: &[TimedWord], turns: &[Turn], fallback: &str) -> Vec<(String, TimedWord)> {
    words
        .iter()
        .map(|w| {
            let mid = (w.start + w.end) / 2.0;
            if let Some(turn) = turns.iter().find(|t| mid >= t.start && mid <= t.end) {
                return (turn.speaker.clone(), w.clone());
            }
            let nearest = turns
                .iter()
                .min_by(|a, b| distance(mid, a).partial_cmp(&distance(mid, b)).unwrap_or(std::cmp::Ordering::Equal));
            match nearest {
                Some(t) if distance(mid, t) < 2.0 => (t.speaker.clone(), w.clone()),
                _ => (fallback.to_string(), w.clone()),
            }
        })
        .collect()
}

/// Consecutive words of the same speaker, close in time, make one line.
pub fn lines(words: &[(String, TimedWord)]) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    for (speaker, word) in words {
        if let Some(last) = out.last_mut() {
            if last.speaker == *speaker && word.start - last.end < PAUSE && word.end - last.start < MAX_LINE_SECONDS {
                last.text.push(' ');
                last.text.push_str(&word.text);
                last.end = word.end;
                continue;
            }
        }
        out.push(Line { id: new_id(), speaker: speaker.clone(), start: word.start, end: word.end, text: word.text.clone() });
    }
    out
}

pub fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

fn overlaps(a: &Line, b: &Line, margin: f64) -> bool {
    a.start < b.end + margin && b.start < a.end + margin
}

/// Without headphones, the others' voices come out of the speakers and into
/// the microphone too. A line of yours whose words the others' recording also
/// has, at the same moment, is that echo.
pub fn removing_echo(mine: &[Line], others: &[Line]) -> Vec<Line> {
    mine.iter()
        .filter(|line| {
            let words = tokens(&line.text);
            if words.len() < 2 {
                let first = words.first().cloned().unwrap_or_default();
                return !others.iter().any(|o| overlaps(o, line, 0.8) && tokens(&o.text).contains(&first));
            }
            let near: Vec<String> = others
                .iter()
                .filter(|o| overlaps(o, line, 1.5))
                .flat_map(|o| tokens(&o.text))
                .collect();
            if near.is_empty() {
                return true;
            }
            let shared = words.iter().filter(|w| near.contains(w)).count();
            (shared as f64) / (words.len() as f64) < 0.6
        })
        .cloned()
        .collect()
}

/// The whole transcript: your lines (echo removed) and the others', in time
/// order, with the speakers named in the order they first speak.
pub fn build(
    mine: &[TimedWord],
    others: &[TimedWord],
    turns: &[Turn],
    me_name: &str,
    person_name: impl Fn(usize) -> String,
) -> (Vec<Speaker>, Vec<Line>) {
    let my_lines = lines(&mine.iter().map(|w| (ME.to_string(), w.clone())).collect::<Vec<_>>());
    let their_lines = lines(&assign(others, turns, "others"));
    let mut all = removing_echo(&my_lines, &their_lines);
    all.extend(their_lines);
    all.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));

    let mut speakers = Vec::new();
    if !my_lines.is_empty() || others.is_empty() {
        speakers.push(Speaker { id: ME.into(), name: me_name.into(), color: 0, is_me: true });
    }
    let mut ids: HashMap<String, String> = HashMap::new();
    let mut renamed = Vec::with_capacity(all.len());
    for mut line in all {
        if line.speaker != ME {
            if let Some(id) = ids.get(&line.speaker) {
                line.speaker = id.clone();
            } else {
                let number = speakers.iter().filter(|s| !s.is_me).count() + 2;
                let id = format!("p{number}");
                ids.insert(line.speaker.clone(), id.clone());
                speakers.push(Speaker { id: id.clone(), name: person_name(number), color: number - 1, is_me: false });
                line.speaker = id;
            }
        }
        renamed.push(line);
    }
    (speakers, renamed)
}

/// One recording for everyone (people around the same computer, no call): the
/// diarizer tells the voices apart and they are all "Person 1", "Person 2"…
pub fn build_room(words: &[TimedWord], turns: &[Turn], person_name: impl Fn(usize) -> String) -> (Vec<Speaker>, Vec<Line>) {
    let raw = lines(&assign(words, turns, "others"));
    let mut speakers: Vec<Speaker> = Vec::new();
    let mut ids: HashMap<String, String> = HashMap::new();
    let named = raw
        .into_iter()
        .map(|mut line| {
            if let Some(id) = ids.get(&line.speaker) {
                line.speaker = id.clone();
            } else {
                let number = speakers.len() + 1;
                let id = format!("p{number}");
                ids.insert(line.speaker.clone(), id.clone());
                speakers.push(Speaker { id: id.clone(), name: person_name(number), color: number, is_me: false });
                line.speaker = id;
            }
            line
        })
        .collect();
    (speakers, named)
}

/// 75 → "1:15", 3725 → "1:02:05".
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The summary, key points and to-dos read from the model's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub title: Option<String>,
    pub summary: String,
    pub key_points: Vec<String>,
    pub todos: Vec<Todo>,
}

pub const SUMMARY_INSTRUCTIONS: &str = "You write the notes of a meeting from its transcript. Each line is \"Name: words\".\n\
Write in the language of the transcript. Invent nothing that was not said.\n\
Reply ONLY with JSON, no code block, in this exact shape:\n\
{\"title\": \"a short title, 6 words at most\", \"summary\": \"2 to 4 sentences: what was decided and why\", \
\"key_points\": [\"up to 6 short points\"], \"todos\": [{\"text\": \"the task\", \"owner\": \"the person's name as written in the transcript, or null\", \"due\": \"the deadline as said, or null\"}]}";

/// Reads the model's JSON answer, even wrapped in a code block or with text
/// around it. None when there is no JSON in it.
pub fn parse_summary(reply: &str) -> Option<Summary> {
    let open = reply.find('{')?;
    let close = reply.rfind('}')?;
    if open >= close {
        return None;
    }
    let json: Value = serde_json::from_str(&reply[open..=close]).ok()?;
    let obj = json.as_object()?;
    let summary = obj.get("summary").and_then(Value::as_str).unwrap_or("").trim().to_string();
    let key_points: Vec<String> = obj
        .get("key_points")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).filter(|s| !s.is_empty()).map(String::from).collect())
        .unwrap_or_default();
    let field = |d: &serde_json::Map<String, Value>, k: &str| -> Option<String> {
        let v = d.get(k)?.as_str()?;
        if v.is_empty() || v.eq_ignore_ascii_case("null") {
            None
        } else {
            Some(v.to_string())
        }
    };
    let todos: Vec<Todo> = obj
        .get("todos")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|item| {
                    if let Some(text) = item.as_str() {
                        return (!text.is_empty()).then(|| Todo { id: new_id(), text: text.into(), owner: None, due: None, done: false });
                    }
                    let d = item.as_object()?;
                    let text = d.get("text")?.as_str()?;
                    if text.is_empty() {
                        return None;
                    }
                    Some(Todo { id: new_id(), text: text.into(), owner: field(d, "owner"), due: field(d, "due"), done: false })
                })
                .collect()
        })
        .unwrap_or_default();
    if summary.is_empty() && key_points.is_empty() && todos.is_empty() {
        return None;
    }
    let title = obj.get("title").and_then(Value::as_str).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    Some(Summary { title, summary, key_points, todos })
}

#[cfg(test)]
mod tests {
    #[test]
    fn brief_orphan_speakers_join_established_turns_without_moving_audio() {
        let original = vec![t("A", 0.0, 8.0), t("fragment-a", 8.0, 8.7), t("B", 20.0, 28.0), t("fragment-b", 28.0, 29.0)];
        let merged = merging_fragments(original.clone());
        assert_eq!(merged.iter().map(|turn| turn.speaker.as_str()).collect::<Vec<_>>(), ["A", "A", "B", "B"]);
        assert_eq!(merged.iter().map(|turn| (turn.start, turn.end)).collect::<Vec<_>>(), original.iter().map(|turn| (turn.start, turn.end)).collect::<Vec<_>>());
    }

    #[test]
    fn fragment_merging_keeps_short_only_calls_and_repeated_brief_speakers() {
        let short = vec![t("A", 0.0, 1.0), t("B", 2.0, 3.0)];
        assert_eq!(merging_fragments(short.clone()), short);
        let mut repeated = vec![t("A", 0.0, 10.0)];
        for i in 0..10 { repeated.push(t("B", 20.0 + i as f64 * 3.0, 22.0 + i as f64 * 3.0)); }
        assert_eq!(merging_fragments(repeated.clone()), repeated);
    }

    #[test]
    fn matching_remote_names_merge_without_rewriting_text_or_microphone_identity() {
        let (mut people, mut transcript) = build(&[w("mine", 0.0, 1.0)], &[w("one", 2.0, 3.0), w("two", 6.0, 7.0)], &[t("A", 2.0, 3.0), t("B", 6.0, 7.0)], "Me", |n| format!("Person {n}"));
        let original_text: Vec<_> = transcript.iter().map(|line| (line.text.clone(), line.start, line.end)).collect();
        rename_speaker(&mut people, &mut transcript, "p2", "Alex");
        rename_speaker(&mut people, &mut transcript, "p3", " alex ");
        assert_eq!(people.len(), 2);
        assert_eq!(transcript[1].speaker, "p2");
        assert_eq!(transcript[2].speaker, "p2");
        assert_eq!(transcript.iter().map(|line| (line.text.clone(), line.start, line.end)).collect::<Vec<_>>(), original_text);
        rename_speaker(&mut people, &mut transcript, ME, "Alex");
        assert_eq!(people.len(), 2);
        assert_eq!(transcript[0].speaker, ME);
        rename_speaker(&mut people, &mut transcript, "p2", " ");
        assert_eq!(people[1].name, "Alex");
    }
    use super::*;

    fn w(text: &str, start: f64, end: f64) -> TimedWord {
        TimedWord { text: text.into(), start, end }
    }

    fn t(speaker: &str, start: f64, end: f64) -> Turn {
        Turn { speaker: speaker.into(), start, end }
    }

    fn person(n: usize) -> String {
        format!("Person {n}")
    }

    #[test]
    fn words_go_to_the_voice_speaking_or_the_nearest_one() {
        let turns = [t("A", 0.0, 2.0), t("B", 5.0, 7.0)];
        let words = [w("hi", 0.5, 0.8), w("there", 3.0, 3.4), w("you", 5.5, 5.8), w("far", 20.0, 20.4)];
        let got: Vec<String> = assign(&words, &turns, "others").into_iter().map(|(s, _)| s).collect();
        assert_eq!(got, ["A", "A", "B", "others"]);
    }

    #[test]
    fn a_pause_or_a_new_speaker_starts_a_new_line() {
        let words = vec![
            ("me".to_string(), w("bonjour", 0.0, 0.4)),
            ("me".to_string(), w("à", 0.5, 0.6)),
            ("me".to_string(), w("tous", 0.7, 0.9)),
            ("me".to_string(), w("alors", 3.0, 3.3)),
            ("p2".to_string(), w("salut", 3.4, 3.7)),
        ];
        let l = lines(&words);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].text, "bonjour à tous");
        assert_eq!((l[0].start, l[0].end), (0.0, 0.9));
    }

    #[test]
    fn the_others_voices_heard_in_the_microphone_are_dropped() {
        let mine = lines(&[
            ("me".to_string(), w("on", 1.0, 1.1)),
            ("me".to_string(), w("se", 1.2, 1.3)),
            ("me".to_string(), w("voit", 1.4, 1.6)),
            ("me".to_string(), w("demain", 1.7, 2.0)),
            ("me".to_string(), w("oui", 10.0, 10.2)),
        ]);
        let others = lines(&[
            ("A".to_string(), w("on", 1.05, 1.15)),
            ("A".to_string(), w("se", 1.25, 1.35)),
            ("A".to_string(), w("voit", 1.45, 1.65)),
            ("A".to_string(), w("demain", 1.75, 2.05)),
        ]);
        let kept = removing_echo(&mine, &others);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "oui");
    }

    #[test]
    fn a_call_is_you_then_the_people_in_the_order_they_speak() {
        let mine = [w("hello", 0.0, 0.5)];
        let others = [w("salut", 2.0, 2.4), w("hey", 6.0, 6.3)];
        let turns = [t("X", 1.5, 3.0), t("Y", 5.5, 7.0)];
        let (speakers, lines) = build(&mine, &others, &turns, "Me", person);
        assert_eq!(speakers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Me", "Person 2", "Person 3"]);
        assert!(speakers[0].is_me);
        assert_eq!(lines.iter().map(|l| l.speaker.as_str()).collect::<Vec<_>>(), ["me", "p2", "p3"]);
        // A room recording numbers everyone from 1.
        let (speakers, _) = build_room(&others, &turns, person);
        assert_eq!(speakers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Person 1", "Person 2"]);
    }

    #[test]
    fn clock_and_summary_parsing() {
        assert_eq!(clock(75.0), "1:15");
        assert_eq!(clock(3725.0), "1:02:05");
        let reply = "Voici :\n```json\n{\"title\":\"Point produit\",\"summary\":\"On lance.\",\"key_points\":[\"a\",\"\"],\"todos\":[{\"text\":\"Envoyer\",\"owner\":\"null\",\"due\":\"vendredi\"},\"Relire\"]}\n```";
        let s = parse_summary(reply).unwrap();
        assert_eq!(s.title.as_deref(), Some("Point produit"));
        assert_eq!(s.key_points, ["a"]);
        assert_eq!(s.todos.len(), 2);
        assert_eq!(s.todos[0].owner, None);
        assert_eq!(s.todos[0].due.as_deref(), Some("vendredi"));
        assert!(parse_summary("no json here").is_none());
        assert!(parse_summary("{\"summary\":\"\"}").is_none());
    }
}
