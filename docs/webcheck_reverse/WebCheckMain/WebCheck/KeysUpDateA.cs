using System;
using System.IO;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.Devices;
using Microsoft.VisualBasic.FileIO;
using WebCheck.My;

namespace WebCheck;

internal class KeysUpDateA
{
	private int ZK;

	private string[] DM;

	public KeysUpDateA()
	{
		ZK = 0;
		DM = new string[1];
	}

	internal bool DownloadKeyFile(string TINstr)
	{
		string text = "http://lic.webchek.com.ua/" + TINstr + ".lic";
		string strPathFile = All.MyDoc() + "\\WebCheck\\Lic\\" + TINstr + ".lic";
		string text2 = All.MyDoc() + "\\WebCheck\\Lic\\temp.lic";
		string text3 = TINstr + ".lic";
		DeleteFil(text2);
		text += "?";
		text += All.A.FN;
		bool result;
		try
		{
			((ServerComputer)MyProject.Computer).Network.DownloadFile(text, text2);
			Application.DoEvents();
			if (File.Exists(text2))
			{
				DeleteFil(strPathFile);
				FileSystem.RenameFile(text2, text3);
				Application.DoEvents();
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_00af;
		}
		result = true;
		goto IL_00af;
		IL_00af:
		return result;
	}

	private void DeleteFil(string strPathFile)
	{
		try
		{
			if (File.Exists(strPathFile))
			{
				FileSystem.DeleteFile(strPathFile);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void DownLoadKeysAll()
	{
		ZK = 0;
		checked
		{
			DM = new string[ZK + 1];
			try
			{
				new KeysWC();
				int num = All.f.IndexMaxFn();
				for (int i = 1; i <= num; i++)
				{
					string text = All.f.NameFn(i).Trim();
					if (text.Length != 10)
					{
						continue;
					}
					string text2 = All.f.StringGetFn(text, "TIN");
					if (!DtinTrue(text2))
					{
						if (DownloadKeyFile(text2))
						{
							ZK++;
							ref string[] dM = ref DM;
							dM = (string[])Utils.CopyArray((Array)dM, (Array)new string[ZK + 1]);
							DM[ZK] = text2;
						}
						Application.DoEvents();
					}
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private bool DtinTrue(string tinS)
	{
		int zK = ZK;
		for (int i = 1; i <= zK; i = checked(i + 1))
		{
			if (Operators.CompareString(DM[i], tinS, false) == 0)
			{
				return true;
			}
		}
		return false;
	}
}
