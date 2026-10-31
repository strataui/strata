// Tree-sitter grammar specification for Strata RSX declarative macro syntax

module.exports = grammar({
  name: 'strata',

  rules: {
    source_file: $ => repeat($._node),

    _node: $ => choice(
      $.element,
      $.self_closing_element,
      $.block,
      $.text
    ),

    element: $ => seq(
      $.open_tag,
      repeat($._node),
      $.close_tag
    ),

    open_tag: $ => seq(
      '<',
      $.tag_name,
      repeat($.attribute),
      '>'
    ),

    close_tag: $ => seq(
      '</',
      $.tag_name,
      '>'
    ),

    self_closing_element: $ => seq(
      '<',
      $.tag_name,
      repeat($.attribute),
      '/>'
    ),

    tag_name: $ => /[a-zA-Z_][a-zA-Z0-9_\-:]*/,

    attribute: $ => seq(
      $.attribute_name,
      optional(seq(
        '=',
        choice($.attribute_value, $.block)
      ))
    ),

    attribute_name: $ => /[a-zA-Z_][a-zA-Z0-9_\-]*/,

    attribute_value: $ => choice(
      seq('"', /[^"]*/, '"'),
      seq("'", /[^']*/, "'")
    ),

    block: $ => seq(
      '{',
      repeat(choice(/[^{}]+/, $.block)),
      '}'
    ),

    text: $ => /[^<{}]+/
  }
});
