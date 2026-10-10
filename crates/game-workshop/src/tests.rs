use super::*;

fn stock(id: u64, good: Good, quantity: u64) -> Inventory {
    Inventory {
        id: InventoryId(id),
        location: InventoryLocation::Area(AreaId(1)),
        quantity: Quantity(quantity),
        capacity: Quantity(20_000),
        access: BTreeSet::from([PersonId(1)]),
        last_granted: None,
        good,
        spoils_at: None,
    }
}
fn site() -> Site {
    Site {
        id: SiteId(5),
        area: AreaId(1),
        input: InventoryId(1),
        input_quantity: Quantity(2_000),
        tool: InventoryId(2),
        output: InventoryId(3),
        output_quantity: Quantity(4_000),
        duration: SimDuration(10_001),
        access: BTreeSet::from([PersonId(1)]),
        last_granted: None,
    }
}
fn facts() -> StartFacts {
    StartFacts {
        actor: PersonId(1),
        area: AreaId(1),
        capable: true,
        adult: true,
        fatigue_allows: true,
        site_access: true,
        occupied_by_other: false,
        input_usable: true,
        input_unreserved: Quantity(2_000),
        tool_usable: true,
        rate: SkillRate {
            numerator: 7,
            denominator: 3,
        },
        body_rate: SkillRate::default(),
    }
}
fn closing(actor: u64, equipped: bool) -> CompletionFacts {
    CompletionFacts {
        actor: PersonId(actor),
        active: true,
        alive_at_start: true,
        equipped,
    }
}
fn start() -> Start {
    site()
        .prepare_start(
            None,
            &stock(1, Good::Timber, 2_000),
            &stock(2, Good::Tool, 1_000),
            &stock(3, Good::Food, 0),
            facts(),
        )
        .unwrap()
}

#[test]
fn admission_requires_real_stock_tools_contact_and_available_space() {
    let site = site();
    let input = stock(1, Good::Timber, 2_000);
    let tool = stock(2, Good::Tool, 1_000);
    let output = stock(3, Good::Food, 0);
    let accepted = site
        .prepare_start(None, &input, &tool, &output, facts())
        .unwrap();
    assert_eq!(
        accepted.consumption,
        Some(Consumption {
            source: input.id,
            quantity: Quantity(2_000),
            expected_input: input.clone(),
            admitted_quantity: Quantity(2_000),
        })
    );
    assert_eq!(input.quantity, Quantity(2_000));
    for denied in [
        StartFacts {
            capable: false,
            ..facts()
        },
        StartFacts {
            adult: false,
            ..facts()
        },
        StartFacts {
            fatigue_allows: false,
            ..facts()
        },
        StartFacts {
            site_access: false,
            ..facts()
        },
        StartFacts {
            occupied_by_other: true,
            ..facts()
        },
        StartFacts {
            input_usable: false,
            ..facts()
        },
        StartFacts {
            input_unreserved: Quantity(1_999),
            ..facts()
        },
        StartFacts {
            tool_usable: false,
            ..facts()
        },
        StartFacts {
            area: AreaId(2),
            ..facts()
        },
    ] {
        assert!(
            site.prepare_start(None, &input, &tool, &output, denied)
                .is_err()
        );
    }
    assert!(
        site.prepare_start(
            None,
            &stock(1, Good::Timber, 1_999),
            &tool,
            &output,
            facts()
        )
        .is_err()
    );
    assert!(
        site.prepare_start(None, &input, &stock(2, Good::Tool, 999), &output, facts())
            .is_err()
    );
    assert!(
        site.prepare_start(None, &input, &tool, &stock(3, Good::Food, 16_001), facts())
            .is_err()
    );
}

#[test]
fn resume_keeps_fractional_work_and_rates_without_consuming_again() {
    let mut job = start().job;
    job.remaining = 77;
    job.work_remainder = 2;
    let resumed = site()
        .prepare_start(
            Some(&job),
            &stock(1, Good::Timber, 0),
            &stock(9, Good::Tool, 1_000),
            &stock(3, Good::Food, 0),
            StartFacts {
                input_usable: false,
                input_unreserved: Quantity(0),
                rate: SkillRate::default(),
                ..facts()
            },
        )
        .unwrap();
    assert_eq!(resumed.consumption, None);
    assert_eq!(resumed.job.remaining, 77);
    assert_eq!(resumed.job.work_remainder, 2);
    assert_eq!(resumed.job.rate, job.rate);
    assert_eq!(resumed.job.tool, Some(InventoryId(9)));
}

#[test]
fn progress_and_completion_boundary_are_slice_equivalent() {
    let mut job = start().job;
    let rate = SkillRate {
        numerator: 7,
        denominator: 3,
    };
    let whole = job.evaluate(333, rate, true, true).unwrap();
    for elapsed in [1, 17, 99, 216] {
        job.apply_progress(job.evaluate(elapsed, rate, true, true).unwrap());
    }
    assert_eq!(job.remaining, whole.remaining);
    assert_eq!(job.work_remainder, whole.remainder);
    let finish = job.completion_after(rate).unwrap();
    assert!(
        job.evaluate(finish - 1, rate, true, true)
            .unwrap()
            .remaining
            > 0
    );
    assert_eq!(job.evaluate(finish, rate, true, true).unwrap().remaining, 0);
}

#[test]
fn paused_or_unequipped_work_does_not_advance_and_cancel_does_not_refund() {
    let mut job = start().job;
    job.work_remainder = 2;
    assert_eq!(
        job.evaluate(100_000, facts().rate, false, true).unwrap(),
        Progress {
            remaining: job.remaining,
            remainder: 2
        }
    );
    assert_eq!(
        job.evaluate(100_000, facts().rate, true, false).unwrap(),
        Progress {
            remaining: job.remaining,
            remainder: 2
        }
    );
    assert!(interrupted(true, false));
    assert!(interrupted(false, true));
    assert!(!interrupted(true, true));
    let consumed_input = stock(1, Good::Timber, 0);
    let mut state = Some(job);
    cancel(&mut state);
    assert!(state.is_none());
    assert_eq!(consumed_input.quantity, Quantity(0));
}

#[test]
fn blocked_completed_job_can_resume_after_output_is_removed() {
    let mut job = start().job;
    job.remaining = 0;
    let site = site();
    let mut output = stock(3, Good::Food, 18_000);
    assert!(
        job.prepare_completion(&site, &output, closing(1, true))
            .unwrap()
            .is_none()
    );
    output.quantity = Quantity(0);
    assert!(
        job.prepare_completion(&site, &output, closing(1, true))
            .unwrap()
            .is_some()
    );
    assert!(
        job.prepare_completion(&site, &output, closing(1, false))
            .unwrap()
            .is_none()
    );
}

#[test]
fn prepared_production_is_atomic_and_carries_producer_attribution() {
    let mut site = site();
    let mut output = stock(3, Good::Food, 5_000);
    let mut job = start().job;
    job.remaining = 0;
    let mut job = Some(job);
    let production = job
        .as_ref()
        .unwrap()
        .prepare_completion(&site, &output, closing(8, true))
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            production.actor,
            production.site,
            production.output,
            production.quantity
        ),
        (PersonId(8), SiteId(5), InventoryId(3), Quantity(4_000))
    );
    let replay = production.clone();
    production.commit(&mut job, &mut site, &mut output).unwrap();
    assert_eq!(output.quantity, Quantity(9_000));
    assert!(job.is_none());
    assert_eq!(site.last_granted, Some(PersonId(8)));
    assert!(replay.commit(&mut job, &mut site, &mut output).is_err());
    assert_eq!(output.quantity, Quantity(9_000));
}

#[test]
fn stale_production_never_partially_changes_job_stock_or_cursor() {
    let mut site = site();
    let mut output = stock(3, Good::Food, 0);
    let mut job = start().job;
    job.remaining = 0;
    let mut job = Some(job);
    let production = job
        .as_ref()
        .unwrap()
        .prepare_completion(&site, &output, closing(1, true))
        .unwrap()
        .unwrap();
    output.quantity = Quantity(1);
    let before = (job.clone(), site.clone(), output.clone());
    assert!(production.commit(&mut job, &mut site, &mut output).is_err());
    assert_eq!((job, site, output), before);
}

#[test]
fn job_serialization_preserves_paused_fractional_work() {
    let mut job = start().job;
    job.work_remainder = 2;
    let encoded = serde_json::to_string(&job).unwrap();
    assert_eq!(serde_json::from_str::<Job>(&encoded).unwrap(), job);
}

#[test]
fn input_effect_only_spends_the_real_admitted_recipe_quantity() {
    let consumption = start().consumption.unwrap();
    let mut input = stock(1, Good::Timber, 2_000);
    assert!(
        consumption
            .commit(&mut stock(7, Good::Timber, 2_000))
            .is_err()
    );
    consumption.commit(&mut input).unwrap();
    assert_eq!(input.quantity, Quantity(0));
    assert!(consumption.commit(&mut input).is_err());
    assert_eq!(input.quantity, Quantity(0));
}

#[test]
fn recipe_or_output_metadata_changes_reject_prepared_production_atomically() {
    let original_site = site();
    let original_output = stock(3, Good::Food, 0);
    let mut finished = start().job;
    finished.remaining = 0;
    for changed in 0..4 {
        let mut site = original_site.clone();
        let mut output = original_output.clone();
        let mut job = Some(finished.clone());
        let production = finished
            .prepare_completion(&site, &output, closing(1, true))
            .unwrap()
            .unwrap();
        match changed {
            0 => site.duration = SimDuration(9),
            1 => output.good = Good::Herb,
            2 => output.location = InventoryLocation::Carried(PersonId(4)),
            _ => output.access.clear(),
        }
        let before = (job.clone(), site.clone(), output.clone());
        assert!(production.commit(&mut job, &mut site, &mut output).is_err());
        assert_eq!((job, site, output), before);
    }
}

#[test]
fn invalid_paused_rate_and_inactive_or_previously_dead_completion_are_rejected() {
    let mut paused = start().job;
    paused.rate.denominator = 0;
    assert!(
        site()
            .prepare_start(
                Some(&paused),
                &stock(1, Good::Timber, 0),
                &stock(2, Good::Tool, 1_000),
                &stock(3, Good::Food, 0),
                facts()
            )
            .is_err()
    );
    let mut finished = start().job;
    finished.remaining = 0;
    for facts in [
        CompletionFacts {
            active: false,
            ..closing(1, true)
        },
        CompletionFacts {
            alive_at_start: false,
            ..closing(1, true)
        },
    ] {
        assert!(
            finished
                .prepare_completion(&site(), &stock(3, Good::Food, 0), facts)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn completion_forecast_uses_evaluated_work_without_creating_output() {
    let job = start().job;
    let site = site();
    let output = stock(3, Good::Food, 0);
    let completed = job
        .evaluate(
            job.completion_after(job.rate).unwrap(),
            job.rate,
            true,
            true,
        )
        .unwrap();
    assert_eq!(
        job.output_forecast(completed, &site, output.good),
        Some((site.id, output.good, site.output_quantity))
    );
    assert_eq!(output.quantity, Quantity(0));
    assert!(job.remaining > 0);
}

#[test]
fn craft_practice_tracks_actual_work_even_when_output_is_blocked() {
    let mut job = start().job;
    job.remaining = 0;
    let practice = job.practice(PersonId(1), 37, true, true).unwrap();
    assert_eq!(
        (
            practice.actor,
            practice.skill,
            practice.milliseconds,
            practice.mastery_cap
        ),
        (PersonId(1), Skill::Craft, 37, None)
    );
    assert!(job.practice(PersonId(1), 0, true, true).is_none());
    assert!(job.practice(PersonId(1), 37, false, true).is_none());
    assert!(job.practice(PersonId(1), 37, true, false).is_none());
}
