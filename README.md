Toy project reading CSV "transactions" into client accounts and printing a summary.

This is probably a bit over-engineered, but I was having fun with it.
In particular, I don't think I needed to handle overflow, I just thought it'd be interesting - and it was!

The program streams the input and output so it can handle large files, but in order to handle disputing deposits of arbitrary age it needs to store all of them.
That could lead to memory issues with very large files, but I prioritized correctness over optimization here.

The program will panic on invalid input.
"Client" errors are reported upwards by the `Client` struct but then just discarded by the caller.

There are unit tests for each module, some of which contain sample data.
