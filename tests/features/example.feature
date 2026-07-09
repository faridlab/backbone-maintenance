# maintenance acceptance oracle
# Flow maps:   docs/business-flows/
# Golden cases: docs/business-flows/golden-cases.md
# Declarative, business-level. Executable truth lives in tests/*.rs.

Feature: Example flow
  In order to <business outcome>
  As a <actor>
  I want to <capability>

  Background:
    Given the tenant schema "maintenance" is migrated

  @happy-path @module:maintenance
  Scenario: Create an example
    When I create an example named "First"
    Then it is persisted with status "active"

  @validation @module:maintenance
  Scenario: A blank name is rejected
    When I create an example with a blank name
    Then the request is rejected with "invalid_name"
