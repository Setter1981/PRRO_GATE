using System.Runtime.InteropServices;

namespace TaxGrpc;

[Guid("73728573-92FD-4B88-8867-D408C1C83E41")]
[ComVisible(true)]
public interface IAnswer
{
	string Id { get; set; }

	string IdData { get; set; }

	string IdSign { get; set; }

	int Status { get; set; }

	string Message { get; set; }
}
