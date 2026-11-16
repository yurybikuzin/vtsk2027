use anyhow::{Context, Result};
use greeners::{CovarianceType, DataFrame, Formula, OLS};
use std::path::Path;

pub struct Args<'a> {
    pub csv: &'a Path,
    pub formula: &'a str,
    pub lags: usize,
}

pub fn run(args: Args) -> Result<()> {
    let df = DataFrame::from_csv(args.csv)
        .with_context(|| format!("read csv {}", args.csv.display()))?;
    let df = df.dropna().context("drop rows with missing values")?;

    let formula = Formula::parse(args.formula)
        .with_context(|| format!("parse formula '{}'", args.formula))?;

    let result = OLS::from_formula(&formula, &df, CovarianceType::NeweyWest(args.lags))
        .context("fit OLS with Newey-West")?;

    println!("formula: {}", args.formula);
    println!("n_obs  : {}", result.n_obs);
    println!("r2     : {:.4}", result.r_squared);
    println!();
    println!(
        "{:<16} {:>12} {:>12} {:>10}",
        "term", "coef", "std_err", "t"
    );
    let names = result.variable_names.clone().unwrap_or_default();
    for i in 0..result.params.len() {
        let name = names.get(i).cloned().unwrap_or_else(|| format!("b{}", i));
        let coef = result.params[i];
        let se = result.std_errors[i];
        let t = result.t_values[i];
        println!("{:<16} {:>12.6} {:>12.6} {:>10.3}", name, coef, se, t);
    }
    Ok(())
}
