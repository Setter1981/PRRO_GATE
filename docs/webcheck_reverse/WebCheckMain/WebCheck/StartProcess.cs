using System;
using System.Diagnostics;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class StartProcess
{
	public void StartNewProcess(string PathEXE)
	{
		try
		{
			Process.Start(PathEXE);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void StartStopProcess(string PathEXE)
	{
		try
		{
			Process process = Process.Start(PathEXE);
			process.WaitForExit();
			process.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}
}
