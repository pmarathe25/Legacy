use toml::Value;

pub(crate) struct Definition {
    pub product: String,
    pub days: u32,
    pub min_celsius: i16,
    pub max_celsius: i16,
}

pub(crate) fn read(object: &Value) -> Result<Option<Definition>, String> {
    let Some(tags) = object.get("tags").and_then(Value::as_array) else {
        return Ok(None);
    };
    let tag = |prefix| -> Result<_, String> {
        let mut matches = tags
            .iter()
            .filter_map(Value::as_str)
            .filter_map(|tag| tag.strip_prefix(prefix));
        let value = matches.next();
        if matches.next().is_some() {
            return Err(format!("duplicate {prefix} tag"));
        }
        Ok(value)
    };
    let product = tag("matures:")?;
    let days = tag("maturation-days:")?;
    let min = tag("maturation-min:")?;
    let max = tag("maturation-max:")?;
    let Some(product) = product else {
        return if days.is_some() || min.is_some() || max.is_some() {
            Err("maturation conditions require a product".into())
        } else {
            Ok(None)
        };
    };
    let days = days
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|&value| value > 0)
        .ok_or("maturation needs a positive duration in days")?;
    let bound = |value: &str| {
        value
            .parse::<i16>()
            .map_err(|_| "invalid maturation temperature")
    };
    let min_celsius = min.map(bound).transpose()?.unwrap_or(i16::MIN);
    let max_celsius = max.map(bound).transpose()?.unwrap_or(i16::MAX);
    if product.is_empty() || min_celsius > max_celsius {
        return Err("invalid maturation product or temperature range".into());
    }
    Ok(Some(Definition {
        product: product.to_owned(),
        days,
        min_celsius,
        max_celsius,
    }))
}
