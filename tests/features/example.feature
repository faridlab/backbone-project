# =============================================================================
# backbone-project — acceptance oracle (declarative)
# =============================================================================
# Business-level spec for the project-delivery bounded context. Declarative only —
# the EXECUTABLE truth lives in the Rust tests; every Scenario below is tagged with
# the test that proves it:
#   @pgc-N    tests/project_golden_cases.rs   (golden cases)
#   @ip-N     tests/integrity_probes.rs        (integrity probes)
#   @pbseam-1 tests/project_billing_seam.rs    (timesheet → REAL billing)
#
# Flow map:   docs/business-flows/
# Golden cases (prose + numbers): docs/business-flows/golden-cases.md
#
# Invariant this module guards: a project POSTS NO GL itself. The only money move is
# bill_timesheet → backbone-billing (which builds the Sales Invoice and posts
# Dr A/R · Cr Service Revenue · Cr PPN). Everything else is roll-ups + state gates.
# =============================================================================

Feature: Project-to-cash — capture, deliver, log effort, bill
  As a service delivery org
  I want to log billable effort against a project and hand it to billing in one controlled step
  So that margin is visible without the project module ever touching the GL

  Background:
    Given the tenant schema "project" is migrated
      And company row-scoping (RLS) is active on every child table
      And an activity type "Consulting" with billing 500000 / costing 300000 per hour

  # --- the roll-up is computed from lines, never stored by hand -----------------
  @pgc-1 @roll-up
  Scenario: Logged effort rolls billable + costing onto the project
    Given an external project "Atlas" for a customer exists
    When I log 8 billable hours + 2 non-billable hours against "Atlas"
    Then the timesheet's hours are 10
      And the timesheet's billable amount is 4,000,000      # billable lines only (8 h × 500000)
      And the timesheet's costing amount is 3,000,000        # ALL lines (10 h × 300000)
      And the project's total_billable_amount is 4,000,000
      And the project's total_costing_amount is 3,000,000
      But the project's total_billed_amount is 0             # nothing billed yet

  # --- the ONE outbound seam: a submitted, billable timesheet → billing ----------
  @pgc-2 @pbseam-1 @billing-seam
  Scenario: Billing a submitted timesheet hands off to billing and rolls total_billed
    Given a submitted timesheet on "Atlas" with a billable amount of 4,000,000
    When I bill the timesheet
    Then billing creates one Sales Invoice for the project's customer at 4,000,000
      And the timesheet is marked "billed" with the echoed invoice_id
      And the project's total_billed_amount is 4,000,000

  @ip-1 @idempotent
  Scenario: Re-billing a timesheet is idempotent
    Given a timesheet that has already been billed
    When I bill it again
    Then no second Sales Invoice is created
      And billing is driven exactly once
      And total_billed_amount is rolled exactly once

  @ip-2 @fails-closed
  Scenario: A wholly non-billable timesheet cannot be billed
    Given a submitted timesheet whose lines are all non-billable
    When I attempt to bill it
    Then billing is refused before any GL is driven
      And the timesheet is not marked "billed"

  @ip-4 @fails-closed
  Scenario: Billable time on a customer-less (internal) project cannot be billed
    Given an internal project with no customer and a submitted billable timesheet
    When I attempt to bill it
    Then billing is refused closed (there is no customer to invoice)

  @ip-5 @non-billable
  Scenario: Only billable lines invoice and roll into billed
    Given a submitted timesheet with 8 billable + 2 non-billable hours
    When I bill the timesheet
    Then only the 8 billable hours are invoiced
      And the 2 non-billable hours contribute to costing but never to billed

  @ip-7 @cancel
  Scenario: Cancelling a timesheet reverses the roll-up
    Given a project whose roll-up was inflated by a fat-fingered 40-hour sheet
    When I cancel that timesheet
    Then the project's roll-up reverses to exactly the good sheet's contribution
      And re-cancelling is a no-op
      But a billed timesheet cannot be cancelled

  @ip-6 @race @maturity
  Scenario: Completing a project while effort is logged conserves the roll-up
    Given log-time and complete_project racing on the same project
    When both resolve
    Then the stored total_billable_amount equals what ProjectCompleted reported
      And no submitted timesheet is stranded on the completed project

  @ip-3 @lifecycle
  Scenario: A completed project is terminal
    Given a completed project
    When I attempt to log time or re-complete it
    Then the attempts are rejected
      And the project stays completed


Feature: Template instantiation — repeatable engagements in one shot
  As a delivery lead
  I want to instantiate a standard blueprint into a project + task tree
  So that a repeatable service is not hand-built every time

  Background:
    Given the tenant schema "project" is migrated

  @pgc-3 @template
  Scenario: A multi-task template stamps out a project with its full task tree
    Given an active template "Standard Onboarding" with 3 ordered tasks
    When I instantiate the template
    Then one project is created
      And the project has exactly 3 tasks in the template's order
      And no GL is posted


Feature: Validation — invalid state is rejected before it is written
  As a module owner
  I want malformed requests rejected with a clear error
  So that invariant-breaking rows never reach the database

  Background:
    Given the tenant schema "project" is migrated

  @pgc-4 @validation
  Scenario Outline: Invariant violations are rejected
    When I attempt to <action>
    Then the request is rejected with "<error>"

    Examples:
      | action                                                 | error          |
      | create an external project with no customer             | needs_customer |
      | submit a timesheet with no lines                        | needs_line     |
      | log a timesheet line with zero or negative hours        | positive_hours |
      | create a task with no subject                           | needs_subject  |
