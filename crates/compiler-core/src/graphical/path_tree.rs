use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub text: String,
    pub location: String,
}

impl Chunk {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            location: String::new(),
        }
    }

    pub fn located(text: impl Into<String>, location: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            location: location.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathNode {
    True,
    Leaf(Vec<Chunk>),
    And(Vec<PathNode>),
    Or(Vec<PathNode>),
}

pub fn leaf(chunks: Vec<Chunk>) -> PathNode {
    PathNode::Leaf(chunks)
}

pub fn path_key(node: &PathNode) -> String {
    match node {
        PathNode::True => "None".to_owned(),
        PathNode::Leaf(chunks) => format!(
            "[{}]",
            chunks
                .iter()
                .map(|chunk| format!("({:?}, {:?})", chunk.text, chunk.location))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PathNode::And(children) => format!(
            "[{}]",
            children.iter().map(path_key).collect::<Vec<_>>().join(", ")
        ),
        PathNode::Or(children) => match children.len() {
            0 => "()".to_owned(),
            1 => format!("({},)", path_key(&children[0])),
            _ => format!(
                "({})",
                children.iter().map(path_key).collect::<Vec<_>>().join(", ")
            ),
        },
    }
}

pub fn stable_sort(nodes: &[PathNode]) -> Vec<PathNode> {
    let mut indexed = nodes.iter().cloned().enumerate().collect::<Vec<_>>();
    indexed.sort_by(|(ai, a), (bi, b)| path_key(a).cmp(&path_key(b)).then(ai.cmp(bi)));
    indexed.into_iter().map(|(_, node)| node).collect()
}

pub fn factorize_paths(paths: &[PathNode]) -> Vec<PathNode> {
    if paths.len() <= 1 {
        return paths.to_vec();
    }
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for path in paths {
        let key = path_key(path);
        if seen.insert(key) {
            unique.push(path.clone());
        }
    }
    if unique.len() <= 1 {
        return unique;
    }
    let sorted = stable_sort(&unique);
    let mut out = Vec::new();
    let mut index = 0;
    while index < sorted.len() {
        let mut end = index + 1;
        while end < sorted.len() && can_factor(&sorted[index], &sorted[end]) {
            end += 1;
        }
        if end - index == 1 {
            out.push(sorted[index].clone());
        } else {
            out.push(factor_group(&sorted[index..end]));
        }
        index = end;
    }
    out
}

fn and_children(node: &PathNode) -> &[PathNode] {
    match node {
        PathNode::And(children) => children,
        _ => std::slice::from_ref(node),
    }
}

fn can_factor(a: &PathNode, b: &PathNode) -> bool {
    let a = and_children(a);
    let b = and_children(b);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    path_key(&a[0]) == path_key(&b[0])
        || path_key(a.last().expect("nonempty")) == path_key(b.last().expect("nonempty"))
}

fn factor_group(group: &[PathNode]) -> PathNode {
    let lists = group.iter().map(and_children).collect::<Vec<_>>();
    let mut head_len = 0;
    while let Some(reference) = lists[0].get(head_len) {
        let key = path_key(reference);
        if lists
            .iter()
            .skip(1)
            .any(|list| list.get(head_len).is_none_or(|node| path_key(node) != key))
        {
            break;
        }
        head_len += 1;
    }

    let mut tail_len = 0;
    loop {
        let min_left = lists
            .iter()
            .map(|list| list.len().saturating_sub(head_len))
            .min()
            .unwrap_or(0);
        if tail_len >= min_left {
            break;
        }
        let reference = &lists[0][lists[0].len() - 1 - tail_len];
        let key = path_key(reference);
        if lists
            .iter()
            .skip(1)
            .any(|list| path_key(&list[list.len() - 1 - tail_len]) != key)
        {
            break;
        }
        tail_len += 1;
    }

    let head = lists[0][..head_len].to_vec();
    let tail = if head_len + tail_len >= lists[0].len() {
        Vec::new()
    } else {
        lists[0][lists[0].len() - tail_len..].to_vec()
    };
    let middles = lists
        .iter()
        .map(|list| {
            let end = list.len().saturating_sub(tail_len);
            let middle = &list[head_len..end];
            match middle.len() {
                0 => PathNode::True,
                1 => middle[0].clone(),
                _ => PathNode::And(middle.to_vec()),
            }
        })
        .collect::<Vec<_>>();
    let mut children = head;
    children.push(PathNode::Or(stable_sort(&middles)));
    children.extend(tail);
    if children.len() == 1 {
        children.remove(0)
    } else {
        PathNode::And(children)
    }
}

pub fn compute_paths(node: &PathNode, first: bool) -> Vec<Chunk> {
    match node {
        PathNode::True => vec![Chunk::plain("TRUE")],
        PathNode::Leaf(chunks) => chunks.clone(),
        PathNode::And(children) => join_groups(
            &children
                .iter()
                .map(|child| compute_paths(child, false))
                .collect::<Vec<_>>(),
            Chunk::plain(" AND "),
        ),
        PathNode::Or(children) => {
            let inner = join_groups(
                &children
                    .iter()
                    .map(|child| compute_paths(child, false))
                    .collect::<Vec<_>>(),
                Chunk::plain(" OR "),
            );
            if first {
                inner
            } else {
                let mut out = vec![Chunk::plain("(")];
                out.extend(inner);
                out.push(Chunk::plain(")"));
                out
            }
        }
    }
}

fn join_groups(groups: &[Vec<Chunk>], separator: Chunk) -> Vec<Chunk> {
    let mut out = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            out.push(separator.clone());
        }
        out.extend(group.clone());
    }
    out
}

pub fn chunks_text(chunks: &[Chunk]) -> String {
    chunks.iter().map(|chunk| chunk.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> PathNode {
        leaf(vec![Chunk::located(name, name)])
    }

    #[test]
    fn factors_common_head_like_openplc() {
        let paths = vec![
            PathNode::And(vec![named("A"), named("B")]),
            PathNode::And(vec![named("A"), named("C")]),
        ];
        let factored = factorize_paths(&paths);
        assert_eq!(factored.len(), 1);
        assert_eq!(
            chunks_text(&compute_paths(&factored[0], true)),
            "A AND (B OR C)"
        );
    }

    #[test]
    fn deduplicates_identical_path_locations() {
        let paths = vec![named("A"), named("A")];
        assert_eq!(factorize_paths(&paths), vec![named("A")]);
    }
}
