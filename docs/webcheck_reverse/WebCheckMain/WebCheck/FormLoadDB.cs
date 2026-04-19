using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Amazon;
using Amazon.Runtime.CredentialManagement;
using Amazon.S3;
using Amazon.S3.Model;
using Ionic.Zip;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.Devices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

[DesignerGenerated]
public class FormLoadDB : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ClB")]
	private Button _ClB;

	private TypLoadDB UpLoadT;

	internal virtual Button ClB
	{
		[CompilerGenerated]
		get
		{
			return _ClB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ClB_Click;
			Button clB = _ClB;
			if (clB != null)
			{
				((Control)clB).Click -= eventHandler;
			}
			_ClB = value;
			clB = _ClB;
			if (clB != null)
			{
				((Control)clB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ErT")]
	internal virtual TextBox ErT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("LastDB")]
	internal virtual Label LastDB
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0001: Unknown result type (might be due to invalid IL or missing references)
		//IL_000b: Expected O, but got Unknown
		//IL_000c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0016: Expected O, but got Unknown
		//IL_0017: Unknown result type (might be due to invalid IL or missing references)
		//IL_0021: Expected O, but got Unknown
		//IL_003e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0048: Expected O, but got Unknown
		//IL_00e1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00eb: Expected O, but got Unknown
		//IL_0189: Unknown result type (might be due to invalid IL or missing references)
		//IL_0193: Expected O, but got Unknown
		ClB = new Button();
		ErT = new TextBox();
		LastDB = new Label();
		((Control)this).SuspendLayout();
		((Control)ClB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ClB).Location = new Point(12, 181);
		((Control)ClB).Name = "ClB";
		((Control)ClB).Size = new Size(610, 44);
		((Control)ClB).TabIndex = 0;
		((ButtonBase)ClB).Text = "Закрити";
		((ButtonBase)ClB).UseVisualStyleBackColor = true;
		((TextBoxBase)ErT).BackColor = SystemColors.Window;
		((Control)ErT).Enabled = false;
		((Control)ErT).Font = new Font("Microsoft Sans Serif", 13.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)ErT).Location = new Point(12, 12);
		ErT.Multiline = true;
		((Control)ErT).Name = "ErT";
		((TextBoxBase)ErT).ReadOnly = true;
		((Control)ErT).Size = new Size(610, 123);
		((Control)ErT).TabIndex = 1;
		ErT.Text = "Чекайте, йде завантаження даних...";
		ErT.TextAlign = (HorizontalAlignment)2;
		LastDB.AutoSize = true;
		((Control)LastDB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)LastDB).Location = new Point(8, 138);
		((Control)LastDB).Name = "LastDB";
		((Control)LastDB).Size = new Size(276, 20);
		((Control)LastDB).TabIndex = 10;
		LastDB.Text = "Останнє успішне завантаження";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(634, 237);
		((Form)this).ControlBox = false;
		((Control)this).Controls.Add((Control)(object)LastDB);
		((Control)this).Controls.Add((Control)(object)ErT);
		((Control)this).Controls.Add((Control)(object)ClB);
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormLoadDB";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Завантаження даних";
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	public FormLoadDB(TypLoadDB e)
	{
		((Form)this).Load += LoadDB_Load;
		UpLoadT = e;
		InitializeComponent();
	}

	private void LoadDB_Load(object sender, EventArgs e)
	{
		((Control)this).Show();
		Application.DoEvents();
		if (UpLoadT.Online)
		{
			((Control)ClB).Enabled = false;
			ErT.Text = "Чекайте, йде завантаження даних...";
			((Control)LastDB).Visible = false;
			Application.DoEvents();
			string pathN = All.MyDoc() + "\\WebCheck\\Archive\\" + UpLoadT.FN + "\\" + UpLoadT.FN + ".db";
			if (TestBackup(pathN))
			{
				UpdateTime();
				((Control)ClB).Enabled = true;
				((Form)this).Close();
			}
			else
			{
				ErT.Text = "Виникла помилка ";
				LastDB.Text = "Остання успішне оновлення: " + UpLoadT.Update;
				((Control)LastDB).Visible = true;
				((Control)ClB).Enabled = true;
			}
		}
		else
		{
			((Control)ClB).Enabled = true;
			ErT.Text = "Доступ заборонено";
			LastDB.Text = "Остання успішне оновлення: " + UpLoadT.Update;
			((Control)LastDB).Visible = true;
		}
	}

	private void ClB_Click(object sender, EventArgs e)
	{
		((Form)this).Close();
	}

	private void UpdateTime()
	{
		AccountantОffice accountantОffice = new AccountantОffice();
		int num = accountantОffice.IndexKeysTin(UpLoadT.Tin, UpLoadT.FN);
		All.ArS.StringWriteFN(UpLoadT.Tin, accountantОffice.NameKeyINI("UP", Conversions.ToInteger(num.ToString())), "S " + DateTime.Now);
	}

	private bool TestBackup(string PathN)
	{
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + UpLoadT.FN + ".db";
		bool result;
		if (!File.Exists(text))
		{
			string text2 = All.PersonalTemp() + "s3.txt";
			if (!File.Exists(text2))
			{
				DownLoadFileS3(text2);
			}
			Coding coding = new Coding();
			IniHGB iniHGB = new IniHGB(text2);
			string keyId = coding.DeCod(iniHGB.GetString("AWS", "KeyId"));
			string secret = coding.DeCod(iniHGB.GetString("AWS", "Secret"));
			WriteProfile(keyId, secret);
			string f = UpLoadT.FN.Trim();
			string t = UpLoadT.Tin.Trim();
			NP nP = default(NP);
			if (!nP.FileArchive(ref f, ref t))
			{
				result = false;
				goto IL_0136;
			}
			if (DownLoadZip(f, t))
			{
				f = All.MyDoc() + "\\WebCheck\\Backup\\" + f + ".zip";
				if (File.Exists(f))
				{
					FileSystem.DeleteFile(f);
				}
			}
		}
		if (File.Exists(text))
		{
			try
			{
				File.Copy(text, PathN, overwrite: true);
				FileSystem.DeleteFile(text);
				result = true;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
		}
		else
		{
			result = false;
		}
		goto IL_0136;
		IL_0136:
		return result;
	}

	internal bool DownLoadZip(string Name, string Key)
	{
		AmazonS3Client amazonS3Client = new AmazonS3Client(RegionEndpoint.EUWest2);
		if (!Directory.Exists(All.MyDoc() + "\\WebCheck\\Backup\\"))
		{
			Directory.CreateDirectory(All.MyDoc() + "\\WebCheck\\Backup\\");
		}
		GetObjectRequest getObjectRequest = new GetObjectRequest();
		getObjectRequest.BucketName = "webchekzipfns";
		getObjectRequest.Key = Name + ".zip";
		_ = null;
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + Name + ".zip";
		bool result;
		try
		{
			GetObjectResponse @object = amazonS3Client.GetObject(getObjectRequest);
			@object.WriteResponseStreamToFile(text);
			if (@object.HttpStatusCode != HttpStatusCode.OK)
			{
				result = false;
				goto IL_00ec;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_00ec;
		}
		using (ZipFile zipFile = ZipFile.Read(text))
		{
			try
			{
				string path = All.MyDoc() + "\\WebCheck\\Backup\\";
				zipFile.Password = Key;
				zipFile.ExtractAll(path);
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				result = false;
				ProjectData.ClearProjectError();
				goto IL_00ec;
			}
		}
		result = true;
		goto IL_00ec;
		IL_00ec:
		return result;
	}

	private bool WriteProfile(string keyId, string secret, string profileName = "default")
	{
		bool result;
		try
		{
			CredentialProfileOptions credentialProfileOptions = new CredentialProfileOptions();
			credentialProfileOptions.AccessKey = keyId;
			credentialProfileOptions.SecretKey = secret;
			CredentialProfile profile = new CredentialProfile(profileName, credentialProfileOptions);
			new NetSDKCredentialsFile().RegisterProfile(profile);
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private bool DownLoadFileS3(string fl)
	{
		//IL_0014: Unknown result type (might be due to invalid IL or missing references)
		string text = "https://s3.eu-west-2.amazonaws.com/che.ck.ua/s3.txt";
		bool result;
		try
		{
			if (File.Exists(fl))
			{
				FileSystem.DeleteFile(fl);
			}
			new Network().DownloadFile(text, fl);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0034;
		}
		result = true;
		goto IL_0034;
		IL_0034:
		return result;
	}
}
