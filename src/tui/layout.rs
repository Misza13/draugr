use std::any::type_name;
use rhai::{Map, Dynamic};
use anyhow::{Context, Result, bail, anyhow};
use ratatui::prelude::*;

use crate::tui::{
    input::*,
    panes::*
};

pub enum LayoutElement {
    VerticalStack {
        children: Vec<LayoutElement>,
        constraints: Vec<Constraint>,
    },
    HorizontalStack {
        children: Vec<LayoutElement>,
        constraints: Vec<Constraint>,
    },
    Pane(LayoutPane),
}

pub enum LayoutPane {
    ScrollPane { id: Option<usize>, pane: ScrollPane, },
    // StaticPane { id: Option<usize>, pane: StaticPane, },
    InputPane(InputPane),
}

impl LayoutElement {
    pub fn from(layout: Map) -> Result<LayoutElement> {
        /* TODO:
         * - move over buffers, if given
         */
        let element_type: String = layout.get("type")
            .convert("layout element type")?;

        match element_type.as_str() {
            "vstack" => {
                let (children, constraints) = parse_container(layout)
                    .context("Parse vstack container")?;

                Ok(LayoutElement::VerticalStack { children, constraints })
            },
            "hstack" => {
                let (children, constraints) = parse_container(layout)
                    .context("Parse hstack container")?;

                Ok(LayoutElement::HorizontalStack { children, constraints })
            },
            "scroll" => {
                let id = if let Some(id) = layout.get("id") {
                    Some(id.as_int()
                        .map_err(|err| anyhow!(err))
                        .context("Parse pane id as int")? as usize)
                } else {
                    None
                };

                Ok(LayoutElement::Pane(LayoutPane::ScrollPane {
                    id,
                    pane: ScrollPane::new(1000)
                }))
            },
            /*"static" => {
                let id = if let Some(id) = layout.get("id") {
                    Some(id.as_int()
                        .map_err(|err| anyhow!(err))
                        .context("Parse pane id as int")? as usize)
                } else {
                    None
                };

                Ok(LayoutElement::Pane(LayoutPane::StaticPane {
                    id,
                    pane: StaticPane {},
                }))
            },*/
            "input" => {
                Ok(LayoutElement::Pane(LayoutPane::InputPane(InputPane::new())))
            }
            _ => {
                bail!("Invalid layout element type: {element_type}");
            },
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect, active_pane: usize) {
        match self {
            LayoutElement::VerticalStack { children, constraints } => {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(constraints.clone())
                    .split(area);

                for (i, child) in children.iter_mut().enumerate() {
                    child.render(frame, chunks[i], active_pane);
                }
            },
            LayoutElement::HorizontalStack { children, constraints } => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(constraints.clone())
                    .split(area);

                for (i, child) in children.iter_mut().enumerate() {
                    child.render(frame, chunks[i], active_pane);
                }
            },
            LayoutElement::Pane(pane) => match pane {
                LayoutPane::ScrollPane { id, pane } => {
                    pane.render(frame, area, *id, *id == Some(active_pane));
                },
                LayoutPane::InputPane(input_pane) => {
                    input_pane.render(frame, area);
                },
                // LayoutPane::StaticPane { id: _, pane: _ } => { /* TODO */},
            },
        }

    }

    pub fn pane(&mut self, pane_id: usize) -> Option<&mut ScrollPane> {
        match self {
            LayoutElement::HorizontalStack { children, constraints: _ } => {
                children.iter_mut().find_map(|child| child.pane(pane_id))
            },
            LayoutElement::VerticalStack { children, constraints: _ } => {
                children.iter_mut().find_map(|child| child.pane(pane_id))
            },
            LayoutElement::Pane(LayoutPane::ScrollPane { id: Some(id), pane }) if pane_id == *id => {
                Some(pane)
            },
            _ => { None },
        }
    }

    pub fn input(&mut self) -> Option<&mut InputPane> {
        match self {
            LayoutElement::HorizontalStack { children, constraints: _ } => {
                children.iter_mut().find_map(|child| child.input())
            },
            LayoutElement::VerticalStack { children, constraints: _ } => {
                children.iter_mut().find_map(|child| child.input())
            },
            LayoutElement::Pane(LayoutPane::InputPane(input_pane)) => {
                Some(input_pane)
            },
            _ => { None },
        }
    }
}

fn parse_container(layout: Map) -> Result<(Vec<LayoutElement>, Vec<Constraint>)> {
    let children = get_array_property(
        &layout,
        "children",
        create_layout_element)
        .context("Parse container's children")?;

    let constraints = get_array_property(
        &layout,
        "constraints",
        create_constraint)
        .context("Parse container's constraints")?;

    Ok((children, constraints))
}

fn get_array_property<T>(layout: &Map, property_name: &str, mapper: impl Fn(&Dynamic) -> Result<T>) -> Result<Vec<T>> {
    let items: Vec<_> = layout.get(property_name)
        .convert(format!("property \"{property_name}\"").as_str())?;

    let mut result = vec![];

    for item in items {
        let item = mapper(&item)
            .context(format!("Parse item: {item}"))?;

        result.push(item);
    }

    Ok(result)
}

fn create_layout_element(item: &Dynamic) -> Result<LayoutElement> {
    let map = item.clone().try_cast::<Map>()
        .context("Cast layout element to map")?;

    LayoutElement::from(map)
        .context("Create layout element")
}


fn create_constraint(item: &Dynamic) -> Result<Constraint> {
    let constraint: Vec<_> = Some(item)
        .convert("constraint")?;

    let constraint_type: String = constraint.get(0)
        .convert("constraint type")?;

    match constraint_type.as_str() {
        "max" => {
            let max_value: i64 = constraint.get(1)
                .convert("constraint max value")?;

            Ok(Constraint::Max(max_value as u16))
        },
        "min" => {
            let min_value: i64 = constraint.get(1)
                .convert("constraint min value")?;

            Ok(Constraint::Min(min_value as u16))
        }
        "percentage" => {
            let prc_value: i64 = constraint.get(1)
                .convert("constraint percentage value")?;

            Ok(Constraint::Percentage(prc_value as u16))
        }
        _ => {
            bail!("Invalid constraint type: {}", constraint_type);
        }
    }
}

trait DynamicExt {
    fn convert<T: Clone + 'static>(self, what: &str) -> Result<T>;
}

impl DynamicExt for Option<&Dynamic> {
    /// Cast a (maybe) `Dynamic` into a given type.
    /// Returns `Err` if the argument is None, as well as if the cast fails.
    fn convert<T: Clone + 'static>(self, what: &str) -> Result<T> {
        self
            .context(format!("Get {what}"))?
            .clone()
            .try_cast::<T>()
            .context(format!("Get {what} as {}", type_name::<T>()))
    }
}