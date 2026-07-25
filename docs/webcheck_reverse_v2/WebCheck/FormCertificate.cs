using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormCertificate : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("StartTest")]
	private Button _StartTest;

	[CompilerGenerated]
	[AccessedThroughProperty("KeyB")]
	private Button _KeyB;

	[CompilerGenerated]
	[AccessedThroughProperty("SelSwrver")]
	private Button _SelSwrver;

	[CompilerGenerated]
	[AccessedThroughProperty("CopyB")]
	private Button _CopyB;

	[CompilerGenerated]
	[AccessedThroughProperty("DelSert")]
	private Button _DelSert;

	internal virtual Button StartTest
	{
		[CompilerGenerated]
		get
		{
			return _StartTest;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = StartTest_Click;
			Button startTest = _StartTest;
			if (startTest != null)
			{
				startTest.Click -= value2;
			}
			_StartTest = value;
			startTest = _StartTest;
			if (startTest != null)
			{
				startTest.Click += value2;
			}
		}
	}

	internal virtual Button KeyB
	{
		[CompilerGenerated]
		get
		{
			return _KeyB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = KeyB_Click;
			Button keyB = _KeyB;
			if (keyB != null)
			{
				keyB.Click -= value2;
			}
			_KeyB = value;
			keyB = _KeyB;
			if (keyB != null)
			{
				keyB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label11")]
	internal virtual Label Label11
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label10")]
	internal virtual Label Label10
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PasOpT")]
	internal virtual TextBox PasOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("KeyOpT")]
	internal virtual TextBox KeyOpT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Server")]
	internal virtual TextBox Server
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SelSwrver
	{
		[CompilerGenerated]
		get
		{
			return _SelSwrver;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = SelSwrver_Click;
			Button selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				selSwrver.Click -= value2;
			}
			_SelSwrver = value;
			selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				selSwrver.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label21")]
	internal virtual Label Label21
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("CertificateT")]
	internal virtual TextBox CertificateT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button CopyB
	{
		[CompilerGenerated]
		get
		{
			return _CopyB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = CopyB_Click;
			Button copyB = _CopyB;
			if (copyB != null)
			{
				copyB.Click -= value2;
			}
			_CopyB = value;
			copyB = _CopyB;
			if (copyB != null)
			{
				copyB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button DelSert
	{
		[CompilerGenerated]
		get
		{
			return _DelSert;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = DelSert_Click;
			Button delSert = _DelSert;
			if (delSert != null)
			{
				delSert.Click -= value2;
			}
			_DelSert = value;
			delSert = _DelSert;
			if (delSert != null)
			{
				delSert.Click += value2;
			}
		}
	}

	public FormCertificate()
	{
		base.Load += FormCertificate_Load;
		InitializeComponent();
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormCertificate));
		this.StartTest = new System.Windows.Forms.Button();
		this.KeyB = new System.Windows.Forms.Button();
		this.Label11 = new System.Windows.Forms.Label();
		this.Label10 = new System.Windows.Forms.Label();
		this.PasOpT = new System.Windows.Forms.TextBox();
		this.KeyOpT = new System.Windows.Forms.TextBox();
		this.Server = new System.Windows.Forms.TextBox();
		this.SelSwrver = new System.Windows.Forms.Button();
		this.Label21 = new System.Windows.Forms.Label();
		this.CertificateT = new System.Windows.Forms.TextBox();
		this.CopyB = new System.Windows.Forms.Button();
		this.Label1 = new System.Windows.Forms.Label();
		this.DelSert = new System.Windows.Forms.Button();
		base.SuspendLayout();
		this.StartTest.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.StartTest.Location = new System.Drawing.Point(22, 145);
		this.StartTest.Name = "StartTest";
		this.StartTest.Size = new System.Drawing.Size(329, 48);
		this.StartTest.TabIndex = 1;
		this.StartTest.Text = "Перевірити";
		this.StartTest.UseVisualStyleBackColor = true;
		this.KeyB.Location = new System.Drawing.Point(596, 17);
		this.KeyB.Name = "KeyB";
		this.KeyB.Size = new System.Drawing.Size(53, 30);
		this.KeyB.TabIndex = 25;
		this.KeyB.Text = "...";
		this.KeyB.UseVisualStyleBackColor = true;
		this.Label11.AutoSize = true;
		this.Label11.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label11.Location = new System.Drawing.Point(17, 60);
		this.Label11.Name = "Label11";
		this.Label11.Size = new System.Drawing.Size(202, 25);
		this.Label11.TabIndex = 29;
		this.Label11.Text = "Пароль ключа ЕЦП *";
		this.Label10.AutoSize = true;
		this.Label10.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label10.Location = new System.Drawing.Point(17, 20);
		this.Label10.Name = "Label10";
		this.Label10.Size = new System.Drawing.Size(121, 25);
		this.Label10.TabIndex = 28;
		this.Label10.Text = "Ключ ЕЦП *";
		this.PasOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PasOpT.Location = new System.Drawing.Point(258, 57);
		this.PasOpT.Name = "PasOpT";
		this.PasOpT.PasswordChar = '*';
		this.PasOpT.Size = new System.Drawing.Size(329, 30);
		this.PasOpT.TabIndex = 27;
		this.PasOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.KeyOpT.Enabled = false;
		this.KeyOpT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.KeyOpT.Location = new System.Drawing.Point(258, 17);
		this.KeyOpT.Name = "KeyOpT";
		this.KeyOpT.Size = new System.Drawing.Size(329, 30);
		this.KeyOpT.TabIndex = 26;
		this.KeyOpT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Server.Enabled = false;
		this.Server.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Server.Location = new System.Drawing.Point(258, 100);
		this.Server.Name = "Server";
		this.Server.Size = new System.Drawing.Size(328, 30);
		this.Server.TabIndex = 33;
		this.Server.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.SelSwrver.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SelSwrver.Location = new System.Drawing.Point(596, 100);
		this.SelSwrver.Name = "SelSwrver";
		this.SelSwrver.Size = new System.Drawing.Size(53, 30);
		this.SelSwrver.TabIndex = 32;
		this.SelSwrver.Text = "...";
		this.SelSwrver.UseVisualStyleBackColor = true;
		this.Label21.AutoSize = true;
		this.Label21.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label21.Location = new System.Drawing.Point(22, 103);
		this.Label21.Name = "Label21";
		this.Label21.Size = new System.Drawing.Size(77, 25);
		this.Label21.TabIndex = 31;
		this.Label21.Text = "АЦСК *";
		this.CertificateT.Enabled = false;
		this.CertificateT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CertificateT.Location = new System.Drawing.Point(22, 220);
		this.CertificateT.Multiline = true;
		this.CertificateT.Name = "CertificateT";
		this.CertificateT.ReadOnly = true;
		this.CertificateT.Size = new System.Drawing.Size(627, 131);
		this.CertificateT.TabIndex = 34;
		this.CertificateT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.CopyB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CopyB.Location = new System.Drawing.Point(211, 361);
		this.CopyB.Name = "CopyB";
		this.CopyB.Size = new System.Drawing.Size(438, 30);
		this.CopyB.TabIndex = 35;
		this.CopyB.Text = "Скопіювати iдентифікатор ключа суб’єкта";
		this.CopyB.UseVisualStyleBackColor = true;
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 9f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(19, 199);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(217, 18);
		this.Label1.TabIndex = 36;
		this.Label1.Text = "Ідентифікатор ключа суб’єкта";
		this.DelSert.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.DelSert.Location = new System.Drawing.Point(375, 145);
		this.DelSert.Name = "DelSert";
		this.DelSert.Size = new System.Drawing.Size(274, 48);
		this.DelSert.TabIndex = 38;
		this.DelSert.Text = "Видалити сертифікат";
		this.DelSert.UseVisualStyleBackColor = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(667, 405);
		base.Controls.Add(this.DelSert);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.CopyB);
		base.Controls.Add(this.CertificateT);
		base.Controls.Add(this.Server);
		base.Controls.Add(this.SelSwrver);
		base.Controls.Add(this.Label21);
		base.Controls.Add(this.KeyB);
		base.Controls.Add(this.Label11);
		base.Controls.Add(this.Label10);
		base.Controls.Add(this.PasOpT);
		base.Controls.Add(this.KeyOpT);
		base.Controls.Add(this.StartTest);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormCertificate";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Сертифікат";
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormCertificate_Load(object sender, EventArgs e)
	{
	}

	private void StartTest_Click(object sender, EventArgs e)
	{
		All.RetriesPrt = 1;
		All.SF.SignatureStart();
		All.SF.ErrorShow(ShowWindows: true);
		TypErrStrCert typErrStrCert = All.SF.Cert(KeyOpT.Text.Trim(), PasOpT.Text.Trim());
		if (typErrStrCert.errCode == 0)
		{
			CertificateT.Text = typErrStrCert.ReturnStr;
			Clipboard.SetText(CertificateT.Text);
			if (!All.CertificateTrue(typErrStrCert.ReturnSerial))
			{
				Interaction.MsgBox("Ваш сертифікат відкликано!", MsgBoxStyle.Critical, "Увага!");
			}
		}
		else
		{
			CertificateT.Text = "ПОМИЛКА";
			Clipboard.SetText(CertificateT.Text);
		}
		All.SF.SignatureStop();
	}

	private void KeyB_Click(object sender, EventArgs e)
	{
		string text = PathKey();
		if (Operators.CompareString(text, "", TextCompare: false) == 0)
		{
			return;
		}
		KeyOpT.Text = text;
		string left = KeyTip(text);
		if (Operators.CompareString(left, "zs2", TextCompare: false) != 0)
		{
			if (Operators.CompareString(left, "jks", TextCompare: false) == 0)
			{
				All.A.AcskSettingsTemp = 4;
				Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
				All.A.AcskSettings = All.A.AcskSettingsTemp;
			}
		}
		else
		{
			All.A.AcskSettingsTemp = 2;
			Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
			All.A.AcskSettings = All.A.AcskSettingsTemp;
		}
	}

	private string PathKey()
	{
		OpenFileDialog openFileDialog = new OpenFileDialog();
		openFileDialog.Filter = "Key Files|*.dat;*.pfx;*.zs2;*.pk8;*.jks|All Files|*.*";
		if (openFileDialog.ShowDialog() == DialogResult.OK)
		{
			return openFileDialog.FileName;
		}
		return "";
	}

	private string KeyTip(string FilePath)
	{
		FilePath = FilePath.Trim();
		string text = "";
		checked
		{
			try
			{
				text = Conversions.ToString(FilePath[FilePath.Trim().Length - 3]);
				text += Conversions.ToString(FilePath[FilePath.Trim().Length - 2]);
				text += Conversions.ToString(FilePath[FilePath.Trim().Length - 1]);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				text = "";
				ProjectData.ClearProjectError();
			}
			return text.ToLower();
		}
	}

	private void SelSwrver_Click(object sender, EventArgs e)
	{
		new FormServerSelection(NewBase: true).ShowDialog();
		Server.Text = All.SF.Servers(All.A.AcskSettingsTemp).Name;
		All.A.AcskSettings = All.A.AcskSettingsTemp;
	}

	private void CopyB_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(CertificateT.Text);
	}

	private void DelSert_Click(object sender, EventArgs e)
	{
		DelSert.Enabled = false;
		All.RetriesPrt = 1;
		All.SF.SignatureStart();
		All.SF.ErrorShow(ShowWindows: true);
		TypErr typErr = All.SF.CertDel(KeyOpT.Text.Trim(), PasOpT.Text.Trim());
		if (typErr.errCode > 0)
		{
			Interaction.MsgBox("Помилка видалення сертифіката: " + typErr.errStr, MsgBoxStyle.Critical, "Увага!");
		}
		else
		{
			Interaction.MsgBox("Сертифікат видалено!", MsgBoxStyle.OkOnly, "Видалення сертифікату");
		}
		All.SF.SignatureStop();
		DelSert.Enabled = true;
	}
}
