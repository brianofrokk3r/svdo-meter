#!/usr/bin/env python3
import argparse
import sys


def calculate(operation, left, right):
    if operation == "add":
        return left + right
    if operation == "subtract":
        return left - right
    if operation == "multiply":
        return left * right
    if operation == "divide":
        if right == 0:
            raise ValueError("division by zero")
        return left / right
    raise ValueError(f"unsupported operation: {operation}")


def parse_number(value):
    try:
        parsed = float(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError(f"invalid number: {value}") from error
    return parsed


def format_number(value):
    if value.is_integer():
        return str(int(value))
    return str(value)


def build_parser():
    parser = argparse.ArgumentParser(description="Run a calculator operation.")
    parser.add_argument("operation", choices=["add", "subtract", "multiply", "divide"])
    parser.add_argument("left", type=parse_number)
    parser.add_argument("right", type=parse_number)
    return parser


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        result = calculate(args.operation, args.left, args.right)
    except ValueError as error:
        parser.error(str(error))
    print(format_number(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
