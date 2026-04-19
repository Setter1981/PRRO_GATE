using System;
using System.IO;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class LaunchСontrol
{
	private StreamWriter myWriteFile;

	private readonly string[] MestoF;

	public LaunchСontrol()
	{
		int num = All.f.IndexMaxFn();
		MestoF = new string[checked(num + 1)];
	}

	internal int YGet(string fn)
	{
		int result;
		try
		{
			int num = All.f.IndexMaxFn();
			for (int num2 = 1; num2 <= num; num2 = checked(num2 + 1))
			{
				if (Operators.CompareString(MestoF[num2], "0", false) == 0)
				{
					MestoF[num2] = fn;
					result = num2;
					goto IL_004d;
				}
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = 0;
			ProjectData.ClearProjectError();
			goto IL_004d;
		}
		result = 0;
		goto IL_004d;
		IL_004d:
		return result;
	}

	internal void YClear(int index, string fn)
	{
		if (Operators.CompareString(MestoF[index], fn, false) == 0)
		{
			MestoF[index] = "";
		}
	}

	internal int Y(int index)
	{
		int num = All.f.IntegerGetFn(All.A.FN, "IndicatorY");
		int num2 = All.f.IntegerGetFn(All.A.FN, "IndicatorStepY");
		int num3 = All.f.IntegerGetFn(All.A.FN, "IndicatorVisible");
		if (num3 > 0)
		{
			num3 = 0;
			All.f.IntigerWriteFN(All.A.FN, "IndicatorVisible", 1);
		}
		else
		{
			num3 = 18000;
			All.f.IntigerWriteFN(All.A.FN, "IndicatorVisible", 0);
		}
		if (num < 1)
		{
			num = 200;
			All.f.IntigerWriteFN(All.A.FN, "IndicatorY", num);
		}
		if (num2 < 1)
		{
			num2 = 3;
			All.f.IntigerWriteFN(All.A.FN, "IndicatorStepY", num2);
		}
		checked
		{
			num += num3;
			return num + (index - 1) * (50 + num2);
		}
	}

	internal void StartControlForm(string fnControl)
	{
		try
		{
			if (OpenFileForm(fnControl))
			{
				((Control)new FormTimer(fnControl)).Show();
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private bool OpenFileForm(string fnControl)
	{
		bool result;
		try
		{
			string path = All.MyDoc() + "\\WebCheck\\Temp\\All\\" + fnControl + ".wcf";
			StreamWriter streamWriter = new StreamWriter(path, append: false);
			streamWriter.WriteLine(fnControl);
			int num = 0;
			do
			{
				streamWriter.Flush();
				streamWriter.Close();
				Application.DoEvents();
				if (!KilFile(fnControl))
				{
					num = checked(num + 1);
					continue;
				}
				break;
			}
			while (num <= 108);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_005f;
		}
		result = true;
		goto IL_005f;
		IL_005f:
		return result;
	}

	private bool KilFile(string FnT)
	{
		bool result;
		try
		{
			string path = All.MyDoc() + "\\WebCheck\\Temp\\All\\" + FnT + ".wcf";
			if (File.Exists(path))
			{
				File.Delete(path);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0038;
		}
		result = true;
		goto IL_0038;
		IL_0038:
		return result;
	}

	internal TypErr StartControl(string fnControl)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		return result;
	}

	internal void StopControl()
	{
		if (myWriteFile != null)
		{
			try
			{
				myWriteFile.Flush();
				myWriteFile.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	~LaunchСontrol()
	{
		StopControl();
		base.Finalize();
	}
}
