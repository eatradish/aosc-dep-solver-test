mod pkgversion;

use std::{collections::BTreeMap, fmt::Display};

use eyre::Result;
use libaosc::packages::{FetchPackages, Package};
use pkgversion::PkgVersion;
use varisat::{dimacs::DimacsParser, Solver};

const SYMBOL: &str = "<>=!";
const USELESS_SYMBOL: &str = "() ";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DepStmt(String, String, String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Pkg<'a> {
    name: &'a str,
    version: &'a str,
    deps: Vec<DepStmt>,
}

impl Display for DepStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {} {}", self.0, self.1, self.2)
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let pkg = args.next().unwrap();
    let version = args.next();

    let client = FetchPackages::new(true, ".", Some("https://mirrors.bfsu.edu.cn/anthon/debs"));
    let pkgs_amd64 = client.fetch_packages("amd64", "stable")?;
    let pkgs_all = client.fetch_packages("all", "stable")?;
    let pkgs_all = pkgs_all.get_packages();
    let mut pkgs = pkgs_amd64.get_packages().to_owned();
    pkgs.extend(pkgs_all.to_vec());
    pkgs.sort_by(|a, b| a.package.cmp(&b.package));

    let mut v = vec![];
    let mut query_pkgs = pkgs.iter().filter(|x| x.package == pkg).collect::<Vec<_>>();

    let query_pkg = if let Some(version) = version {
        query_pkgs
            .into_iter()
            .find(|x| x.version == version)
            .unwrap()
    } else {
        query_pkgs.sort_by(|a, b| {
            PkgVersion::try_from(a.version.as_str())
                .unwrap()
                .cmp(&PkgVersion::try_from(b.version.as_str()).unwrap())
        });

        query_pkgs.last().unwrap()
    };

    let deps = insert_pkg(query_pkg, &mut v);
    loop_insert(&pkgs, deps, &mut v);

    for i in &v {
        if i.deps.is_empty() {
            continue;
        }

        println!("{} {}:", i.name, i.version);
        for j in &i.deps {
            println!("  {j}");
        }
    }

    let mut map = BTreeMap::new();

    let mut count_value = 0;
    let mut lines_len = 0;

    for i in &v {
        let key = DepStmt(
            i.name.to_string(),
            "=".to_string(),
            i.version.to_string(),
        );

        if !map.contains_key(&key) {
            count_value += 1;
            map.insert(
                DepStmt(
                    i.name.to_string(),
                    "=".to_string(),
                    i.version.to_string(),
                ),
                count_value,
            );
        }

        for j in &i.deps {
            if !map.contains_key(j) {
                count_value += 1;
                map.insert(j.to_owned(), count_value);
            }
        }
    }

    let mut cnf = vec![];

    for i in v{
        let main = *map.get(&DepStmt(
            i.name.to_string(),
            "=".to_string(),
            i.version.to_string(),
        )).unwrap();

        for j in &i.deps {
            cnf.push(-(main));
            cnf.push(*map.get(j).unwrap() as i64);
            cnf.push(0);
            lines_len += 1;
        }
    }

    let mut stmt = String::from("p cnf ");
    stmt.push_str(&count_value.to_string());
    stmt.push_str(" ");
    stmt.push_str(&lines_len.to_string());
    stmt.push('\n');

    for i in &cnf {
        stmt.push_str(&i.to_string());

        if *i == 0 {
            stmt.push('\n');
        } else {
            stmt.push(' ');
        }
    }

    let formula = DimacsParser::parse(stmt.as_bytes()).expect("parse error");

    let mut solver = Solver::new();
    solver.add_formula(&formula);

    let solution = solver.solve().unwrap();
    let model = solver.model().unwrap();

    println!("{}", stmt);

    dbg!(solution);
    dbg!(&model);

    for (k, v) in map {
        println!("{v}: {k}");
    }

    Ok(())
}

fn loop_insert<'a>(pkgs: &'a [Package], deps: Vec<DepStmt>, v: &mut Vec<Pkg<'a>>) {
    if deps.is_empty() {
        return;
    }

    for i in &deps {
        if v.iter().any(|x| x.name == i.0) {
            continue;
        }

        let pkg_versions = pkgs.iter().filter(|x| x.package == i.0);
        for j in pkg_versions {
            let deps = insert_pkg(&j, v);
            loop_insert(pkgs, deps, v);
        }
    }
}

fn insert_pkg<'a>(query_pkg: &'a Package, v: &mut Vec<Pkg<'a>>) -> Vec<DepStmt> {
    let deps = query_pkg
        .depends
        .to_owned()
        .unwrap_or_default()
        .split(", ")
        .map(|x| {
            if !x.is_empty() {
                Some(dep_to_stmt(x))
            } else {
                None
            }
        })
        .flatten()
        .collect::<Vec<_>>();

    v.push(Pkg {
        name: &query_pkg.package,
        version: &query_pkg.version,
        deps: deps.clone(),
    });

    deps
}

fn dep_to_stmt(s: &str) -> DepStmt {
    let mut package = String::new();
    let mut op = String::new();
    let mut version = String::new();
    let mut is_op = false;
    for i in s.chars() {
        if USELESS_SYMBOL.contains(i) {
            continue;
        }

        if !SYMBOL.contains(i) && !is_op {
            package.push(i);
            continue;
        }

        if SYMBOL.contains(i) {
            is_op = true;
            op.push(i);
            continue;
        }

        if is_op && !SYMBOL.contains(i) {
            version.push(i);
        }
    }

    DepStmt(package, op, version)
}
